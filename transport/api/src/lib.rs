//! The crate aggregates and composes individual transport level objects and functionality
//! into a unified [`crate::HoprTransport`] object with the goal of isolating the transport layer
//! and defining a fully specified transport API.
//!
//! As such, the transport layer components should be only those that are directly needed in
//! order to:
//! 1. send and receive a packet, acknowledgement or ticket aggregation request
//! 2. send and receive a network telemetry request
//! 3. automate transport level processes
//! 4. algorithms associated with the transport layer operational management
//! 5. interface specifications to allow modular behavioral extensions

/// Configuration of the [crate::HoprTransport].
pub mod config;
/// Constants used and exposed by the crate.
pub mod constants;
/// Errors used by the crate.
pub mod errors;
pub mod helpers;
pub mod network_notifier;

use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

use async_lock::RwLock;
use futures::{
    channel::mpsc::{UnboundedReceiver, UnboundedSender},
    future::{select, Either},
    pin_mut, FutureExt, StreamExt,
};
use tracing::{debug, error, info, trace, warn};

use core_network::{
    heartbeat::Heartbeat,
    ping::{PingConfig, PingQueryReplier, Pinger, Pinging},
    PeerId,
};
use core_path::path::TransportPath;
use hopr_async_runtime::prelude::{sleep, spawn, JoinHandle};
use hopr_db_sql::{
    api::tickets::{AggregationPrerequisites, HoprDbTicketOperations},
    HoprDbAllOperations,
};
use hopr_internal_types::prelude::*;
use hopr_platform::time::native::current_time;
use hopr_primitive_types::prelude::*;
use hopr_transport_p2p::{
    swarm::{TicketAggregationRequestType, TicketAggregationResponseType},
    HoprSwarm,
};
use hopr_transport_protocol::{
    errors::ProtocolError,
    msg::processor::{MsgSender, PacketInteractionConfig, PacketSendFinalizer},
    ticket_aggregation::processor::{
        AwaitingAggregator, TicketAggregationActions, TicketAggregationInteraction, TicketAggregatorTrait,
    },
};
pub use {
    core_network::network::{Health, Network, NetworkTriggeredEvent, PeerOrigin, PeerStatus},
    hopr_crypto_types::{
        keypairs::{ChainKeypair, Keypair, OffchainKeypair},
        types::{HalfKeyChallenge, Hash, OffchainPublicKey},
    },
    hopr_internal_types::protocol::ApplicationData,
    hopr_transport_p2p::{
        libp2p, libp2p::swarm::derive_prelude::Multiaddr, multiaddrs::strip_p2p_protocol,
        swarm::HoprSwarmWithProcessors, PeerDiscovery,
    },
    hopr_transport_protocol::execute_on_tick,
    hopr_transport_session::{
        errors::TransportSessionError, traits::SendMsg, Capability as SessionCapability, PathOptions, Session,
        SessionClientConfig, SessionId, SESSION_USABLE_MTU_SIZE,
    },
};

use crate::{
    constants::{RESERVED_SESSION_TAG_UPPER_LIMIT, RESERVED_SUBPROTOCOL_TAG_UPPER_LIMIT},
    errors::HoprTransportError,
};

pub use crate::helpers::{IndexerTransportEvent, PeerEligibility, TicketStatistics};

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum HoprTransportProcess {
    Heartbeat,
    Swarm,
    ProtocolAckIn,
    ProtocolAckOut,
    ProtocolMsgIn,
    ProtocolMsgOut,
    SessionsManagement,
    BloomFilterSave,
}

#[derive(Debug, Clone)]
pub struct TicketAggregatorProxy<Db>
where
    Db: HoprDbTicketOperations + Send + Sync + Clone + std::fmt::Debug,
{
    db: Db,
    maybe_writer:
        Arc<std::sync::OnceLock<TicketAggregationActions<TicketAggregationResponseType, TicketAggregationRequestType>>>,
    agg_timeout: std::time::Duration,
}

impl<Db> TicketAggregatorProxy<Db>
where
    Db: HoprDbTicketOperations + Send + Sync + Clone + std::fmt::Debug,
{
    pub fn new(
        db: Db,
        maybe_writer: Arc<
            std::sync::OnceLock<TicketAggregationActions<TicketAggregationResponseType, TicketAggregationRequestType>>,
        >,
        agg_timeout: std::time::Duration,
    ) -> Self {
        Self {
            db,
            maybe_writer,
            agg_timeout,
        }
    }
}

#[async_trait::async_trait]
impl<Db> TicketAggregatorTrait for TicketAggregatorProxy<Db>
where
    Db: HoprDbTicketOperations + Send + Sync + Clone + std::fmt::Debug,
{
    async fn aggregate_tickets(
        &self,
        channel: &Hash,
        prerequisites: AggregationPrerequisites,
    ) -> hopr_transport_protocol::errors::Result<()> {
        if let Some(writer) = self.maybe_writer.clone().get() {
            AwaitingAggregator::new(self.db.clone(), writer.clone(), self.agg_timeout)
                .aggregate_tickets(channel, prerequisites)
                .await
        } else {
            Err(ProtocolError::TransportError(
                "Ticket aggregation writer not available, the object was not yet initialized".to_string(),
            ))
        }
    }
}

pub struct HoprTransportConfig {
    pub transport: config::TransportConfig,
    pub network: core_network::config::NetworkConfig,
    pub protocol: hopr_transport_protocol::config::ProtocolConfig,
    pub heartbeat: core_network::heartbeat::HeartbeatConfig,
}

/// Interface into the physical transport mechanism allowing all off-chain HOPR related tasks on
/// the transport, as well as off-chain ticket manipulation.
pub struct HoprTransport<T>
where
    T: HoprDbAllOperations + std::fmt::Debug + Clone + Send + Sync + 'static,
{
    me: PeerId,
    me_onchain: Address,
    cfg: HoprTransportConfig,
    db: T,
    ping: Arc<OnceLock<Pinger<network_notifier::PingExternalInteractions<T>>>>,
    network: Arc<Network<T>>,
    path_planner: helpers::PathPlanner<T>,
    my_multiaddresses: Vec<Multiaddr>,
    process_packet_send: Arc<OnceLock<MsgSender>>,
    process_ticket_aggregate:
        Arc<OnceLock<TicketAggregationActions<TicketAggregationResponseType, TicketAggregationRequestType>>>,
    sessions: moka::future::Cache<SessionId, UnboundedSender<Box<[u8]>>>,
}

impl<T> HoprTransport<T>
where
    T: HoprDbAllOperations + std::fmt::Debug + Clone + Send + Sync + 'static,
{
    pub fn new(
        me: &OffchainKeypair,
        me_onchain: &ChainKeypair,
        cfg: HoprTransportConfig,
        db: T,
        channel_graph: Arc<RwLock<core_path::channel_graph::ChannelGraph>>,
        my_multiaddresses: Vec<Multiaddr>,
    ) -> Self {
        let identity: libp2p::identity::Keypair = (me).into();

        Self {
            me: identity.public().to_peer_id(),
            me_onchain: me_onchain.public().to_address(),
            db: db.clone(),
            ping: Arc::new(OnceLock::new()),
            network: Arc::new(Network::new(
                me.public().into(),
                my_multiaddresses.clone(),
                cfg.network.clone(),
                db.clone(),
            )),
            cfg,
            path_planner: helpers::PathPlanner::new(db, channel_graph),
            my_multiaddresses,
            process_packet_send: Arc::new(OnceLock::new()),
            process_ticket_aggregate: Arc::new(OnceLock::new()),
            sessions: moka::future::Cache::builder()
                .max_capacity(u16::MAX as u64)
                .time_to_idle(std::time::Duration::from_secs(5 * 60))
                .build(),
        }
    }

    pub fn me(&self) -> &PeerId {
        &self.me
    }

    pub fn network(&self) -> Arc<Network<T>> {
        self.network.clone()
    }

    /// Execute all processes of the [`crate::HoprTransport`] object.
    ///
    /// This method will spawn the [`crate::HoprTransportProcess::Heartbeat`], [`crate::HoprTransportProcess::BloomFilterSave`],
    /// [`crate::HoprTransportProcess::Swarm`] and [`crate::HoprTransportProcess::SessionsRouter`] processes and return
    /// join handles to the calling function. These processes are not started immediately, but are
    /// waiting for a trigger from this piece of code.
    #[allow(clippy::too_many_arguments)]
    pub async fn run(
        &self,
        me: &OffchainKeypair,
        me_onchain: &ChainKeypair,
        version: String,
        network: Arc<Network<T>>,
        tbf_path: String,
        on_transport_output: UnboundedSender<ApplicationData>,
        on_acknowledged_ticket: UnboundedSender<AcknowledgedTicket>,
        transport_updates: UnboundedReceiver<PeerDiscovery>,
        incoming_session_queue: UnboundedSender<Session>,
    ) -> HashMap<HoprTransportProcess, JoinHandle<()>> {
        let mut processes: HashMap<HoprTransportProcess, JoinHandle<()>> = HashMap::new();

        // network event processing channel
        let (network_events_tx, network_events_rx) = futures::channel::mpsc::channel::<NetworkTriggeredEvent>(
            constants::MAXIMUM_NETWORK_UPDATE_EVENT_QUEUE_SIZE,
        );

        // manual ping
        let (ping_tx, ping_rx) = futures::channel::mpsc::unbounded::<(PeerId, PingQueryReplier)>();

        let ping_cfg = PingConfig {
            timeout: self.cfg.protocol.heartbeat.timeout,
            ..PingConfig::default()
        };

        let ping: Pinger<network_notifier::PingExternalInteractions<T>> = Pinger::new(
            ping_cfg,
            ping_tx.clone(),
            network_notifier::PingExternalInteractions::new(
                network.clone(),
                self.db.clone(),
                self.path_planner.channel_graph(),
                network_events_tx,
            ),
        );

        self.ping
            .clone()
            .set(ping)
            .expect("must set the ping executor only once");

        let ticket_agg_proc = TicketAggregationInteraction::new(self.db.clone(), me_onchain);
        let tkt_agg_writer = ticket_agg_proc.writer();

        let transport_layer = HoprSwarm::new(
            me.into(),
            network_events_rx,
            transport_updates,
            ping_rx,
            ticket_agg_proc,
            self.my_multiaddresses.clone(),
            self.cfg.protocol,
        )
        .await;

        let (external_msg_send, external_msg_rx) =
            futures::channel::mpsc::unbounded::<(ApplicationData, TransportPath, PacketSendFinalizer)>();

        self.process_packet_send
            .clone()
            .set(MsgSender::new(external_msg_send))
            .expect("must set the packet processing writer only once");

        self.process_ticket_aggregate
            .clone()
            .set(tkt_agg_writer.clone())
            .expect("must set the ticket aggregation writer only once");

        // heartbeat
        let mut heartbeat = Heartbeat::new(
            self.cfg.heartbeat,
            self.ping
                .get()
                .expect("Ping should be initialized at this point")
                .clone(),
            core_network::heartbeat::HeartbeatExternalInteractions::new(network.clone()),
            Box::new(|dur| Box::pin(sleep(dur))),
        );

        // initiate the libp2p transport layer
        let (ack_to_send_tx, ack_to_send_rx) = futures::channel::mpsc::unbounded::<(PeerId, Acknowledgement)>();
        let (ack_received_tx, ack_received_rx) = futures::channel::mpsc::unbounded::<(PeerId, Acknowledgement)>();

        let (msg_to_send_tx, msg_to_send_rx) = futures::channel::mpsc::unbounded::<(PeerId, Box<[u8]>)>();
        let (msg_received_tx, msg_received_rx) = futures::channel::mpsc::unbounded::<(PeerId, Box<[u8]>)>();

        let transport_layer = transport_layer.with_processors(
            ack_to_send_rx,
            ack_received_tx,
            msg_to_send_rx,
            msg_received_tx,
            tkt_agg_writer,
        );

        processes.insert(
            HoprTransportProcess::Swarm,
            spawn(transport_layer.run(version, on_acknowledged_ticket.clone())),
        );

        // initiate the msg-ack protocol stack over the wire transport
        let packet_cfg = PacketInteractionConfig::new(me, me_onchain);

        let (tx_from_protocol, rx_from_protocol) = futures::channel::mpsc::unbounded::<ApplicationData>();
        for (k, v) in hopr_transport_protocol::run_msg_ack_protocol(
            packet_cfg,
            self.db.clone(),
            me_onchain,
            Some(tbf_path),
            on_acknowledged_ticket,
            (ack_to_send_tx, ack_received_rx),
            (msg_to_send_tx, msg_received_rx),
            (tx_from_protocol, external_msg_rx),
        )
        .await
        .into_iter()
        {
            processes.insert(
                match k {
                    hopr_transport_protocol::ProtocolProcesses::AckIn => HoprTransportProcess::ProtocolAckIn,
                    hopr_transport_protocol::ProtocolProcesses::AckOut => HoprTransportProcess::ProtocolAckOut,
                    hopr_transport_protocol::ProtocolProcesses::MsgIn => HoprTransportProcess::ProtocolMsgIn,
                    hopr_transport_protocol::ProtocolProcesses::MsgOut => HoprTransportProcess::ProtocolMsgOut,
                    hopr_transport_protocol::ProtocolProcesses::BloomPersist => HoprTransportProcess::BloomFilterSave,
                },
                v,
            );
        }

        // initiate session handling over the msg-ack protocol stack
        let sessions = self.sessions.clone();
        let me = self.me;
        let message_sender = Arc::new(helpers::MessageSender::new(
            self.process_packet_send.clone(),
            self.path_planner.clone(),
        ));

        processes.insert(
            HoprTransportProcess::SessionsManagement,
            spawn(async move {
                let _the_process_should_not_end = StreamExt::filter_map(rx_from_protocol, move |data| {
                    let sessions = sessions.clone();
                    let me = me;
                    let message_sender = message_sender.clone();
                    let incoming_session_queue = incoming_session_queue.clone();

                    async move {
                        if let Some(app_tag) = data.application_tag {
                            const SPECIAL_TAG_HIGHEST_VALUE: u16 = RESERVED_SUBPROTOCOL_TAG_UPPER_LIMIT - 1;
                            const SESSION_TAG_HIGHEST_VALUE: u16 = RESERVED_SESSION_TAG_UPPER_LIMIT - 1;
                            match app_tag {
                                0..=SPECIAL_TAG_HIGHEST_VALUE => None,
                                RESERVED_SUBPROTOCOL_TAG_UPPER_LIMIT..=SESSION_TAG_HIGHEST_VALUE => {
                                    if let Ok((peer, data)) =
                                        hopr_transport_session::types::unwrap_offchain_key(data.plain_text.clone())
                                    {
                                        if let Some(sender) = sessions.get(&SessionId::new(app_tag, peer)).await {
                                            trace!(
                                                app_tag,
                                                peer_id = tracing::field::debug(peer),
                                                "Received data for a registered session"
                                            );
                                            if let Err(e) = sender.unbounded_send(data) {
                                                error!("Failed to send data to session: {e}");
                                            }
                                        } else {
                                            info!(
                                                app_tag,
                                                peer_id = tracing::field::debug(peer),
                                                "Detected a new incoming session"
                                            );
                                            let session_id = SessionId::new(app_tag, peer);

                                            let (tx, rx) = futures::channel::mpsc::unbounded::<Box<[u8]>>();

                                            if incoming_session_queue
                                                .unbounded_send(Session::new(
                                                    session_id,
                                                    me,
                                                    PathOptions::IntermediatePath(vec![]),
                                                    vec![],
                                                    message_sender.clone(),
                                                    rx,
                                                ))
                                                .is_ok()
                                            {
                                                // if the data does not get into the session, it can recover
                                                if let Err(e) = tx.unbounded_send(data) {
                                                    error!("Failed to send data to session: {e}");
                                                }

                                                sessions.insert(session_id, tx).await;
                                            } else {
                                                warn!("Failed to send session to incoming session queue");
                                            }
                                        }
                                    }
                                    None
                                }
                                _ => Some(data),
                            }
                        } else {
                            Some(data)
                        }
                    }
                })
                .map(Ok)
                .forward(on_transport_output)
                .await;
            }),
        );

        // initiate the network telemetry
        processes.insert(
            HoprTransportProcess::Heartbeat,
            spawn(async move { heartbeat.heartbeat_loop().await }),
        );

        processes
    }

    pub fn ticket_aggregator(&self) -> Arc<dyn TicketAggregatorTrait + Send + Sync + 'static> {
        Arc::new(TicketAggregatorProxy::new(
            self.db.clone(),
            self.process_ticket_aggregate.clone(),
            self.cfg.protocol.ticket_aggregation.timeout,
        ))
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn ping(&self, peer: &PeerId) -> errors::Result<Option<std::time::Duration>> {
        if !self.is_allowed_to_access_network(peer).await? {
            return Err(HoprTransportError::Api(format!(
                "ping to '{peer}' not allowed due to network registry"
            )));
        }

        if peer == &self.me {
            return Err(HoprTransportError::Api("ping to self does not make sense".into()));
        }

        let pinger = self
            .ping
            .get()
            .ok_or_else(|| HoprTransportError::Api("ping processing is not yet initialized".into()))?;

        let timeout = sleep(std::time::Duration::from_secs(30)).fuse();
        let ping = (*pinger).ping(vec![*peer]).fuse();

        pin_mut!(timeout, ping);

        if let Err(e) = self.network.add(peer, PeerOrigin::ManualPing, vec![]).await {
            error!("Failed to store the peer observation: {e}");
        }

        let start = current_time().as_unix_timestamp();

        match select(timeout, ping).await {
            Either::Left(_) => {
                warn!(peer = peer.to_string(), "Manual ping to peer timed out");
                return Err(ProtocolError::Timeout.into());
            }
            Either::Right(_) => info!("Manual ping succeeded"),
        };

        Ok(self
            .network
            .get(peer)
            .await?
            .map(|status| status.last_seen.as_unix_timestamp().saturating_sub(start)))
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn new_session(&self, cfg: SessionClientConfig) -> errors::Result<Session> {
        // TODO: 2.2 session initiation protocol is necessary to establish an application tag instead of this random approach
        let mut session_id: Option<SessionId> = None;
        for _ in 0..100 {
            let random_app_tag = hopr_crypto_random::random_integer(
                RESERVED_SUBPROTOCOL_TAG_UPPER_LIMIT as u64,
                Some(RESERVED_SESSION_TAG_UPPER_LIMIT as u64),
            ) as u16;
            let id = SessionId::new(random_app_tag, cfg.peer);
            if !self.sessions.contains_key(&id) {
                session_id = Some(id);
            }
        }

        let session_id = session_id
            .ok_or_else(|| errors::HoprTransportError::Api("Failed to generate a non-occupied session ID".into()))?;

        debug!(
            session_id = tracing::field::debug(session_id),
            "Generated a new session ID"
        );

        let (tx, rx) = futures::channel::mpsc::unbounded::<Box<[u8]>>();

        self.sessions.insert(session_id, tx).await;

        Ok(Session::new(
            session_id,
            self.me,
            cfg.path_options,
            cfg.capabilities,
            Arc::new(helpers::MessageSender::new(
                self.process_packet_send.clone(),
                self.path_planner.clone(),
            )),
            rx,
        ))
    }

    #[tracing::instrument(level = "info", skip(self, msg), fields(uuid = uuid::Uuid::new_v4().to_string()))]
    pub async fn send_message(
        &self,
        msg: Box<[u8]>,
        destination: PeerId,
        options: PathOptions,
        application_tag: Option<u16>,
    ) -> errors::Result<()> {
        if let Some(application_tag) = application_tag {
            if application_tag < RESERVED_SESSION_TAG_UPPER_LIMIT {
                return Err(HoprTransportError::Api(format!(
                    "Application tag must not be lower than {RESERVED_SESSION_TAG_UPPER_LIMIT}"
                )));
            }
        }

        if msg.len() > PAYLOAD_SIZE {
            return Err(HoprTransportError::Api(format!(
                "Message exceeds the maximum allowed size of {PAYLOAD_SIZE} bytes"
            )));
        }

        let app_data = ApplicationData::new_from_owned(application_tag, msg)?;

        let path = self.path_planner.resolve_path(destination, options).await?;

        let sender = self.process_packet_send.get().ok_or_else(|| {
            HoprTransportError::Api("send msg: failed because message processing is not yet initialized".into())
        })?;

        sender
            .send_packet(app_data, path)
            .await
            .map_err(|e| HoprTransportError::Api(format!("send msg failed to enqueue msg: {e}")))?
            .consume_and_wait(crate::constants::PACKET_QUEUE_TIMEOUT_MILLISECONDS)
            .await
            .map_err(|e| HoprTransportError::Api(e.to_string()))
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn aggregate_tickets(&self, channel_id: &Hash) -> errors::Result<()> {
        let entry = self
            .db
            .get_channel_by_id(None, channel_id)
            .await
            .map_err(hopr_db_sql::api::errors::DbError::from)
            .map_err(HoprTransportError::from)
            .and_then(|c| {
                if let Some(c) = c {
                    Ok(c)
                } else {
                    Err(ProtocolError::ChannelNotFound.into())
                }
            })?;

        if entry.status != ChannelStatus::Open {
            return Err(ProtocolError::ChannelClosed.into());
        }

        Ok(Arc::new(TicketAggregatorProxy::new(
            self.db.clone(),
            self.process_ticket_aggregate.clone(),
            self.cfg.protocol.ticket_aggregation.timeout,
        ))
        .aggregate_tickets(&entry.get_id(), Default::default())
        .await?)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn get_public_nodes(&self) -> errors::Result<Vec<(PeerId, Address, Vec<Multiaddr>)>> {
        Ok(self
            .db
            .get_accounts(None, true)
            .await
            .map_err(hopr_db_sql::api::errors::DbError::from)?
            .into_iter()
            .map(|entry| {
                (
                    PeerId::from(entry.public_key),
                    entry.chain_addr,
                    Vec::from_iter(entry.get_multiaddr().into_iter()),
                )
            })
            .collect())
    }

    pub async fn is_allowed_to_access_network<'a>(&self, peer: &'a PeerId) -> errors::Result<bool>
    where
        T: 'a,
    {
        let db_clone = self.db.clone();
        let peer = *peer;
        Ok(self
            .db
            .begin_transaction()
            .await
            .map_err(hopr_db_sql::api::errors::DbError::from)?
            .perform(|tx| {
                Box::pin(async move {
                    let pk = OffchainPublicKey::try_from(peer)?;
                    if let Some(address) = db_clone.translate_key(Some(tx), pk).await? {
                        db_clone
                            .is_allowed_in_network_registry(Some(tx), address.try_into()?)
                            .await
                    } else {
                        Err(hopr_db_sql::errors::DbSqlError::LogicalError(
                            "cannot translate off-chain key".into(),
                        ))
                    }
                })
            })
            .await
            .map_err(hopr_db_sql::api::errors::DbError::from)?)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn listening_multiaddresses(&self) -> Vec<Multiaddr> {
        self.network
            .get(&self.me)
            .await
            .unwrap_or(None)
            .map(|peer| peer.multiaddresses)
            .unwrap_or(vec![])
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub fn announceable_multiaddresses(&self) -> Vec<Multiaddr> {
        let mut mas = self
            .local_multiaddresses()
            .into_iter()
            .filter(|ma| {
                hopr_transport_p2p::multiaddrs::is_supported(ma)
                    && (self.cfg.transport.announce_local_addresses || !hopr_transport_p2p::multiaddrs::is_private(ma))
            })
            .map(|ma| strip_p2p_protocol(&ma))
            .filter(|v| !v.is_empty())
            .collect::<Vec<_>>();

        mas.sort_by(|l, r| {
            let is_left_dns = hopr_transport_p2p::multiaddrs::is_dns(l);
            let is_right_dns = hopr_transport_p2p::multiaddrs::is_dns(r);

            if !(is_left_dns ^ is_right_dns) {
                std::cmp::Ordering::Equal
            } else if is_left_dns {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            }
        });

        mas
    }

    pub fn local_multiaddresses(&self) -> Vec<Multiaddr> {
        self.my_multiaddresses.clone()
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn network_observed_multiaddresses(&self, peer: &PeerId) -> Vec<Multiaddr> {
        self.network
            .get(peer)
            .await
            .unwrap_or(None)
            .map(|peer| peer.multiaddresses)
            .unwrap_or(vec![])
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn network_health(&self) -> Health {
        self.network.health().await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn network_connected_peers(&self) -> errors::Result<Vec<PeerId>> {
        Ok(self.network.peer_filter(|peer| async move { Some(peer.id.1) }).await?)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn network_peer_info(&self, peer: &PeerId) -> errors::Result<Option<PeerStatus>> {
        Ok(self.network.get(peer).await?)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn ticket_statistics(&self) -> errors::Result<TicketStatistics> {
        let ticket_stats = self.db.get_ticket_statistics(None).await?;

        Ok(TicketStatistics {
            winning_count: ticket_stats.winning_tickets,
            unredeemed_value: ticket_stats.unredeemed_value,
            redeemed_value: ticket_stats.redeemed_value,
            neglected_value: ticket_stats.neglected_value,
            rejected_value: ticket_stats.rejected_value,
        })
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn tickets_in_channel(&self, channel_id: &Hash) -> errors::Result<Option<Vec<AcknowledgedTicket>>> {
        if let Some(channel) = self
            .db
            .get_channel_by_id(None, channel_id)
            .await
            .map_err(hopr_db_sql::api::errors::DbError::from)?
        {
            if channel.destination == self.me_onchain {
                Ok(Some(self.db.get_tickets((&channel).into()).await?))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn all_tickets(&self) -> errors::Result<Vec<Ticket>> {
        Ok(self
            .db
            .get_all_tickets()
            .await?
            .into_iter()
            .map(|v| v.ticket.leak())
            .collect())
    }
}
