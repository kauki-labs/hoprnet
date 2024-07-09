use std::collections::hash_set::HashSet;
use std::time::{Duration, SystemTime};

use futures::StreamExt;
use hopr_primitive_types::traits::SaturatingSub;
use libp2p_identity::PeerId;

use multiaddr::Multiaddr;
use tracing::debug;

pub use hopr_db_api::peers::{HoprDbPeersOperations, PeerOrigin, PeerSelector, PeerStatus, Stats};
use hopr_platform::time::native::current_time;

use crate::config::NetworkConfig;

#[cfg(all(feature = "prometheus", not(test)))]
use {
    hopr_metrics::metrics::{MultiGauge, SimpleGauge},
    hopr_primitive_types::prelude::*,
};

#[cfg(all(feature = "prometheus", not(test)))]
lazy_static::lazy_static! {
    static ref METRIC_NETWORK_HEALTH: SimpleGauge =
        SimpleGauge::new("hopr_network_health", "Connectivity health indicator").unwrap();
    static ref METRIC_PEERS_BY_QUALITY: MultiGauge =
        MultiGauge::new("hopr_peers_by_quality", "Number different peer types by quality",
            &["type", "quality"],
        ).unwrap();
    static ref METRIC_PEER_COUNT: SimpleGauge =
        SimpleGauge::new("hopr_peer_count", "Number of all peers").unwrap();
    static ref METRIC_NETWORK_HEALTH_TIME_TO_GREEN: SimpleGauge = SimpleGauge::new(
        "hopr_time_to_green_sec",
        "Time it takes for a node to transition to the GREEN network state"
    ).unwrap();
}

/// Network health represented with colors, where green is the best and red
/// is the worst possible observed nework quality.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, strum::Display, strum::EnumString)]
pub enum Health {
    /// Unknown health, on application startup
    Unknown = 0,
    /// No connection, default
    Red = 1,
    /// Low quality connection to at least 1 public relay
    Orange = 2,
    /// High quality connection to at least 1 public relay
    Yellow = 3,
    /// High quality connection to at least 1 public relay and 1 NAT node
    Green = 4,
}

/// Events generated by the [Network] object allowing it
/// to physically interact with external systems,
/// including the transport mechanism.
#[derive(Debug, Clone, PartialEq, strum::Display)]
pub enum NetworkTriggeredEvent {
    CloseConnection(PeerId),
    UpdateQuality(PeerId, f64),
}

/// Calculate the health factor for network from the available stats
fn health_from_stats(stats: &Stats, is_public: bool) -> Health {
    let mut health = Health::Red;

    if stats.bad_quality_public > 0 {
        health = Health::Orange;
    }

    if stats.good_quality_public > 0 {
        health = if is_public || stats.good_quality_non_public > 0 {
            Health::Green
        } else {
            Health::Yellow
        };
    }

    health
}

/// The network object storing information about the running observed state of the network,
/// including peers, connection qualities and updates for other parts of the system.
#[derive(Debug)]
pub struct Network<T>
where
    T: HoprDbPeersOperations + Sync + Send + std::fmt::Debug,
{
    me: PeerId,
    me_addresses: Vec<Multiaddr>,
    am_i_public: bool,
    cfg: NetworkConfig,
    db: T,
    #[cfg(all(feature = "prometheus", not(test)))]
    started_at: Duration,
}

impl<T> Network<T>
where
    T: HoprDbPeersOperations + Sync + Send + std::fmt::Debug,
{
    pub fn new(my_peer_id: PeerId, my_multiaddresses: Vec<Multiaddr>, cfg: NetworkConfig, db: T) -> Self {
        if cfg.quality_offline_threshold < cfg.quality_bad_threshold {
            panic!(
                "Strict requirement failed, bad quality threshold {} must be lower than quality offline threshold {}",
                cfg.quality_bad_threshold, cfg.quality_offline_threshold
            );
        }

        #[cfg(all(feature = "prometheus", not(test)))]
        {
            METRIC_NETWORK_HEALTH.set(0.0);
            METRIC_NETWORK_HEALTH_TIME_TO_GREEN.set(0.0);
            METRIC_PEERS_BY_QUALITY.set(&["public", "high"], 0.0);
            METRIC_PEERS_BY_QUALITY.set(&["public", "low"], 0.0);
            METRIC_PEERS_BY_QUALITY.set(&["nonPublic", "high"], 0.0);
            METRIC_PEERS_BY_QUALITY.set(&["nonPublic", "low"], 0.0);
        }

        Self {
            me: my_peer_id,
            me_addresses: my_multiaddresses,
            am_i_public: true,
            cfg: cfg.clone(),
            db,
            #[cfg(all(feature = "prometheus", not(test)))]
            started_at: current_time().as_unix_timestamp(),
        }
    }

    /// Check whether the PeerId is present in the network
    pub async fn has(&self, peer: &PeerId) -> bool {
        peer == &self.me
            || self.db.get_network_peer(peer).await.is_ok_and(|p| {
                p.map(|peer_status| !self.should_still_be_ignored(&peer_status))
                    .unwrap_or(false)
            })
    }

    /// Add a new peer into the network
    ///
    /// Each peer must have an origin specification.
    pub async fn add(&self, peer: &PeerId, origin: PeerOrigin, mut addrs: Vec<Multiaddr>) -> crate::errors::Result<()> {
        if peer == &self.me {
            return Err(crate::errors::NetworkingError::DisallowedOperationOnOwnPeerIdError);
        }

        if let Some(mut peer_status) = self.db.get_network_peer(peer).await? {
            if !self.should_still_be_ignored(&peer_status) {
                peer_status.ignored = None;
                peer_status.multiaddresses.append(&mut addrs);
                peer_status.multiaddresses = peer_status
                    .multiaddresses
                    .into_iter()
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
                self.db.update_network_peer(peer_status).await?;
            }
        } else {
            debug!("Adding '{peer}' from {origin} with multiaddresses {addrs:?}");

            self.db
                .add_network_peer(
                    peer,
                    origin,
                    addrs,
                    self.cfg.backoff_exponent,
                    self.cfg.quality_avg_window_size,
                )
                .await?;
        }

        #[cfg(all(feature = "prometheus", not(test)))]
        {
            let stats = self.db.network_peer_stats(self.cfg.quality_bad_threshold).await?;
            self.refresh_metrics(&stats)
        }

        Ok(())
    }

    pub async fn get(&self, peer: &PeerId) -> crate::errors::Result<Option<PeerStatus>> {
        if peer == &self.me {
            Ok(Some({
                let mut ps = PeerStatus::new(*peer, PeerOrigin::Initialization, 0.0f64, 2u32);
                ps.multiaddresses.clone_from(&self.me_addresses);
                ps
            }))
        } else {
            Ok(self
                .db
                .get_network_peer(peer)
                .await?
                .filter(|peer_status| !self.should_still_be_ignored(peer_status)))
        }
    }

    /// Remove peer from the network
    pub async fn remove(&self, peer: &PeerId) -> crate::errors::Result<()> {
        if peer == &self.me {
            return Err(crate::errors::NetworkingError::DisallowedOperationOnOwnPeerIdError);
        }

        self.db.remove_network_peer(peer).await?;

        #[cfg(all(feature = "prometheus", not(test)))]
        {
            let stats = self.db.network_peer_stats(self.cfg.quality_bad_threshold).await?;
            self.refresh_metrics(&stats)
        }

        Ok(())
    }

    /// Update the peer record with the observation
    pub async fn update(
        &self,
        peer: &PeerId,
        ping_result: std::result::Result<Duration, ()>,
        version: Option<String>,
    ) -> crate::errors::Result<Option<NetworkTriggeredEvent>> {
        if peer == &self.me {
            return Err(crate::errors::NetworkingError::DisallowedOperationOnOwnPeerIdError);
        }

        if let Some(mut entry) = self.db.get_network_peer(peer).await? {
            if !self.should_still_be_ignored(&entry) {
                entry.ignored = None;
            }

            entry.heartbeats_sent += 1;
            entry.peer_version = version;

            if let Ok(latency) = ping_result {
                entry.last_seen = current_time();
                entry.last_seen_latency = latency;
                entry.heartbeats_succeeded += 1;
                entry.backoff = self.cfg.backoff_min;
                entry.update_quality(1.0_f64.min(entry.get_quality() + self.cfg.quality_step));
            } else {
                entry.backoff = self.cfg.backoff_max.max(entry.backoff.powf(self.cfg.backoff_exponent));
                entry.update_quality(0.0_f64.max(entry.get_quality() - self.cfg.quality_step));

                if entry.get_quality() < (self.cfg.quality_step / 2.0) {
                    return Ok(Some(NetworkTriggeredEvent::CloseConnection(entry.id.1)));
                } else if entry.get_quality() < self.cfg.quality_bad_threshold {
                    entry.ignored = Some(current_time());
                }
            }

            self.db.update_network_peer(entry.clone()).await?;

            #[cfg(all(feature = "prometheus", not(test)))]
            {
                let stats = self.db.network_peer_stats(self.cfg.quality_bad_threshold).await?;
                self.refresh_metrics(&stats)
            }

            Ok(Some(NetworkTriggeredEvent::UpdateQuality(
                entry.id.1,
                entry.get_quality(),
            )))
        } else {
            debug!("Ignoring update request for unknown peer {}", peer);
            Ok(None)
        }
    }

    /// Returns the quality of the network as a network health indicator.
    pub async fn health(&self) -> Health {
        self.db
            .network_peer_stats(self.cfg.quality_bad_threshold)
            .await
            .map(|stats| health_from_stats(&stats, self.am_i_public))
            .unwrap_or(Health::Unknown)
    }

    /// Update the internally perceived network status that is processed to the network health
    #[cfg(all(feature = "prometheus", not(test)))]
    fn refresh_metrics(&self, stats: &Stats) {
        let health = health_from_stats(stats, self.am_i_public);

        if METRIC_NETWORK_HEALTH_TIME_TO_GREEN.get() < 0.5f64 {
            if let Some(ts) = current_time().checked_sub(self.started_at) {
                METRIC_NETWORK_HEALTH_TIME_TO_GREEN.set(ts.as_unix_timestamp().as_secs_f64());
            }
        }
        METRIC_PEER_COUNT.set(stats.all_count() as f64);
        METRIC_PEERS_BY_QUALITY.set(&["public", "high"], stats.good_quality_public as f64);
        METRIC_PEERS_BY_QUALITY.set(&["public", "low"], stats.bad_quality_public as f64);
        METRIC_PEERS_BY_QUALITY.set(&["nonPublic", "high"], stats.good_quality_non_public as f64);
        METRIC_PEERS_BY_QUALITY.set(&["nonPublic", "low"], stats.bad_quality_non_public as f64);
        METRIC_NETWORK_HEALTH.set((health as i32).into());
    }

    // ======
    pub async fn peer_filter<Fut, V, F>(&self, filter: F) -> crate::errors::Result<Vec<V>>
    where
        F: FnMut(PeerStatus) -> Fut,
        Fut: std::future::Future<Output = Option<V>>,
    {
        let stream = self.db.get_network_peers(Default::default(), false).await?;
        futures::pin_mut!(stream);
        Ok(stream.filter_map(filter).collect().await)
    }

    pub async fn find_peers_to_ping(&self, threshold: SystemTime) -> crate::errors::Result<Vec<PeerId>> {
        let stream = self
            .db
            .get_network_peers(PeerSelector::default().with_last_seen_lte(threshold), true)
            .await?;
        futures::pin_mut!(stream);
        let mut data: Vec<PeerStatus> = stream
            .filter_map(|v| async move {
                if v.id.1 == self.me {
                    return None;
                }
                let backoff = v.backoff.powf(self.cfg.backoff_exponent);
                let delay = std::cmp::min(self.cfg.min_delay * (backoff as u32), self.cfg.max_delay);

                if (v.last_seen + delay) < threshold {
                    Some(v)
                } else {
                    None
                }
            })
            .collect()
            .await;

        data.sort_by(|a, b| {
            if a.last_seen < b.last_seen {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            }
        });

        Ok(data.into_iter().map(|peer| peer.id.1).collect())
    }

    pub(crate) fn should_still_be_ignored(&self, peer: &PeerStatus) -> bool {
        peer.ignored
            .map(|t| current_time().saturating_sub(t) < self.cfg.ignore_timeframe)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use crate::network::{Health, Network, NetworkConfig, NetworkTriggeredEvent, PeerOrigin};
    use hopr_crypto_types::keypairs::{ChainKeypair, Keypair, OffchainKeypair};
    use hopr_platform::time::native::current_time;
    use hopr_primitive_types::prelude::AsUnixTimestamp;
    use libp2p_identity::PeerId;
    use std::ops::Add;
    use std::time::Duration;

    #[test]
    fn test_network_health_should_serialize_to_a_proper_string() {
        assert_eq!(format!("{}", Health::Orange), "Orange".to_owned())
    }

    #[test]
    fn test_network_health_should_deserialize_from_proper_string() -> Result<(), Box<dyn std::error::Error>> {
        let parsed: Health = "Orange".parse()?;
        Ok(assert_eq!(parsed, Health::Orange))
    }

    async fn basic_network(my_id: &PeerId) -> Network<hopr_db_sql::db::HoprDb> {
        let mut cfg = NetworkConfig::default();
        cfg.quality_offline_threshold = 0.6;
        Network::new(
            *my_id,
            vec![],
            cfg,
            hopr_db_sql::db::HoprDb::new_in_memory(ChainKeypair::random()).await,
        )
    }

    #[test]
    fn test_network_health_should_be_ordered_numerically_for_hopr_metrics_output() {
        assert_eq!(Health::Unknown as i32, 0);
        assert_eq!(Health::Red as i32, 1);
        assert_eq!(Health::Orange as i32, 2);
        assert_eq!(Health::Yellow as i32, 3);
        assert_eq!(Health::Green as i32, 4);
    }

    #[async_std::test]
    async fn test_network_should_not_be_able_to_add_self_reference() {
        let me = PeerId::random();

        let peers = basic_network(&me).await;

        assert!(peers.add(&me, PeerOrigin::IncomingConnection, vec![]).await.is_err());

        assert_eq!(
            0,
            peers
                .peer_filter(|peer| async move { Some(peer.id) })
                .await
                .unwrap_or(vec![])
                .len()
        );
        assert!(peers.has(&me).await)
    }

    #[async_std::test]
    async fn test_network_should_contain_a_registered_peer() {
        let expected: PeerId = OffchainKeypair::random().public().into();
        let me: PeerId = OffchainKeypair::random().public().into();

        let peers = basic_network(&me).await;

        peers
            .add(&expected, PeerOrigin::IncomingConnection, vec![])
            .await
            .unwrap();

        assert_eq!(
            1,
            peers
                .peer_filter(|peer| async move { Some(peer.id) })
                .await
                .unwrap_or(vec![])
                .len()
        );
        assert!(peers.has(&expected).await)
    }

    #[async_std::test]
    async fn test_network_should_remove_a_peer_on_unregistration() {
        let peer: PeerId = OffchainKeypair::random().public().into();
        let me: PeerId = OffchainKeypair::random().public().into();

        let peers = basic_network(&me).await;

        peers.add(&peer, PeerOrigin::IncomingConnection, vec![]).await.unwrap();

        peers.remove(&peer).await.expect("should not fail on DB remove");

        assert_eq!(
            0,
            peers
                .peer_filter(|peer| async move { Some(peer.id) })
                .await
                .unwrap_or(vec![])
                .len()
        );
        assert!(!peers.has(&peer).await)
    }

    #[async_std::test]
    async fn test_network_should_ingore_heartbeat_updates_for_peers_that_were_not_registered() {
        let peer: PeerId = OffchainKeypair::random().public().into();
        let me: PeerId = OffchainKeypair::random().public().into();

        let peers = basic_network(&me).await;

        peers
            .update(&peer, Ok(current_time().as_unix_timestamp()), None)
            .await
            .expect("no error should occur");

        assert_eq!(
            0,
            peers
                .peer_filter(|peer| async move { Some(peer.id) })
                .await
                .unwrap_or(vec![])
                .len()
        );
        assert!(!peers.has(&peer).await)
    }

    #[async_std::test]
    async fn test_network_should_be_able_to_register_a_succeeded_heartbeat_result() {
        let peer: PeerId = OffchainKeypair::random().public().into();
        let me: PeerId = OffchainKeypair::random().public().into();

        let peers = basic_network(&me).await;

        peers.add(&peer, PeerOrigin::IncomingConnection, vec![]).await.unwrap();

        let latency = 123u64;

        peers
            .update(&peer, Ok(std::time::Duration::from_millis(latency)), None)
            .await
            .expect("no error should occur");

        let actual = peers.get(&peer).await.expect("peer record should be present").unwrap();

        assert_eq!(actual.heartbeats_sent, 1);
        assert_eq!(actual.heartbeats_succeeded, 1);
        assert_eq!(actual.last_seen_latency, std::time::Duration::from_millis(latency));
    }

    #[async_std::test]
    async fn test_network_update_should_merge_metadata() {
        let peer: PeerId = OffchainKeypair::random().public().into();
        let me: PeerId = OffchainKeypair::random().public().into();

        let peers = basic_network(&me).await;

        let expected_version = Some("1.2.4".to_string());

        {
            peers
                .add(&peer, PeerOrigin::IncomingConnection, vec![])
                .await
                .expect("should not fail on DB add");
            peers
                .update(&peer, Ok(current_time().as_unix_timestamp()), expected_version.clone())
                .await
                .expect("no error should occur");

            let status = peers.get(&peer).await.unwrap().unwrap();

            assert_eq!(status.peer_version, expected_version);
        }

        let ts = current_time().as_unix_timestamp();

        {
            let expected_version = Some("2.0.0".to_string());

            peers
                .update(&peer, Ok(ts), expected_version.clone())
                .await
                .expect("no error should occur");

            let status = peers
                .get(&peer)
                .await
                .expect("the peer status should be preent")
                .unwrap();

            assert_eq!(status.peer_version, expected_version);
        }
    }

    #[async_std::test]
    async fn test_network_should_ignore_a_peer_that_has_reached_lower_thresholds_a_specified_amount_of_time() {
        let peer: PeerId = OffchainKeypair::random().public().into();
        let me: PeerId = OffchainKeypair::random().public().into();

        let peers = basic_network(&me).await;

        peers.add(&peer, PeerOrigin::IncomingConnection, vec![]).await.unwrap();

        peers
            .update(&peer, Ok(current_time().as_unix_timestamp()), None)
            .await
            .expect("no error should occur");
        peers
            .update(&peer, Ok(current_time().as_unix_timestamp()), None)
            .await
            .expect("no error should occur");
        peers.update(&peer, Err(()), None).await.expect("no error should occur"); // should drop to ignored

        // peers.update(&peer, Err(()), None).await.expect("no error should occur");    // should drop from network

        assert!(!peers.has(&peer).await);

        // peer should remain ignored and not be added
        peers.add(&peer, PeerOrigin::IncomingConnection, vec![]).await.unwrap();

        assert!(!peers.has(&peer).await)
    }

    #[async_std::test]
    async fn test_network_should_be_able_to_register_a_failed_heartbeat_result() {
        let peer: PeerId = OffchainKeypair::random().public().into();
        let me: PeerId = OffchainKeypair::random().public().into();

        let peers = basic_network(&me).await;

        peers.add(&peer, PeerOrigin::IncomingConnection, vec![]).await.unwrap();

        // Needs to do 3 pings, so we get over the ignore threshold limit
        // when doing the 4th failed ping
        peers
            .update(&peer, Ok(std::time::Duration::from_millis(123_u64)), None)
            .await
            .expect("no error should occur");
        peers
            .update(&peer, Ok(std::time::Duration::from_millis(200_u64)), None)
            .await
            .expect("no error should occur");
        peers
            .update(&peer, Ok(std::time::Duration::from_millis(200_u64)), None)
            .await
            .expect("no error should occur");

        peers.update(&peer, Err(()), None).await.expect("no error should occur");

        let actual = peers
            .get(&peer)
            .await
            .unwrap()
            .expect("the peer record should be present");

        assert_eq!(actual.heartbeats_succeeded, 3);
        assert_eq!(actual.backoff, 300f64);
    }

    #[async_std::test]
    async fn test_network_peer_should_be_listed_for_the_ping_if_last_recorded_later_than_reference() {
        let first: PeerId = OffchainKeypair::random().public().into();
        let second: PeerId = OffchainKeypair::random().public().into();
        let me: PeerId = OffchainKeypair::random().public().into();

        let peers = basic_network(&me).await;

        peers.add(&first, PeerOrigin::IncomingConnection, vec![]).await.unwrap();
        peers
            .add(&second, PeerOrigin::IncomingConnection, vec![])
            .await
            .unwrap();

        let latency = 77_u64;

        let mut expected = vec![first, second];
        expected.sort();

        peers
            .update(&first, Ok(std::time::Duration::from_millis(latency)), None)
            .await
            .expect("no error should occur");
        peers
            .update(&second, Ok(std::time::Duration::from_millis(latency)), None)
            .await
            .expect("no error should occur");

        // assert_eq!(
        //     format!(
        //         "{:?}",
        //         peers.should_still_be_ignored(&peers.get(&first).await.unwrap().unwrap())
        //     ),
        //     ""
        // );
        // assert_eq!(format!("{:?}", peers.get(&first).await), "");

        let mut actual = peers
            .find_peers_to_ping(current_time().add(Duration::from_secs(2u64)))
            .await
            .unwrap();
        actual.sort();

        assert_eq!(actual, expected);
    }

    #[async_std::test]
    async fn test_network_should_have_red_health_without_any_registered_peers() {
        let me: PeerId = OffchainKeypair::random().public().into();

        let peers = basic_network(&me).await;

        assert_eq!(peers.health().await, Health::Red);
    }

    #[async_std::test]
    async fn test_network_should_be_unhealthy_without_any_heartbeat_updates() {
        let peer: PeerId = OffchainKeypair::random().public().into();
        let me: PeerId = OffchainKeypair::random().public().into();

        let peers = basic_network(&me).await;

        peers.add(&peer, PeerOrigin::IncomingConnection, vec![]).await.unwrap();

        // all peers are public
        assert_eq!(peers.health().await, Health::Orange);
    }

    #[async_std::test]
    async fn test_network_should_be_unhealthy_without_any_peers_once_the_health_was_known() {
        let peer: PeerId = OffchainKeypair::random().public().into();
        let me: PeerId = OffchainKeypair::random().public().into();

        let peers = basic_network(&me).await;

        peers.add(&peer, PeerOrigin::IncomingConnection, vec![]).await.unwrap();
        let _ = peers.health();
        peers.remove(&peer).await.expect("should not fail on DB remove");

        assert_eq!(peers.health().await, Health::Red);
    }

    #[async_std::test]
    async fn test_network_should_be_healthy_when_a_public_peer_is_pingable_with_low_quality() {
        let peer: PeerId = OffchainKeypair::random().public().into();
        let me: PeerId = OffchainKeypair::random().public().into();

        let mut cfg = NetworkConfig::default();
        cfg.quality_offline_threshold = 0.6;

        let peers = Network::new(
            me,
            vec![],
            cfg,
            hopr_db_sql::db::HoprDb::new_in_memory(ChainKeypair::random()).await,
        );

        peers.add(&peer, PeerOrigin::IncomingConnection, vec![]).await.unwrap();

        peers
            .update(&peer, Ok(current_time().as_unix_timestamp()), None)
            .await
            .expect("no error should occur");

        assert_eq!(peers.health().await, Health::Orange);
    }

    #[async_std::test]
    async fn test_network_should_close_connection_to_peer_once_it_reaches_the_lowest_possible_quality() {
        let peer: PeerId = OffchainKeypair::random().public().into();
        let public = peer;
        let me: PeerId = OffchainKeypair::random().public().into();

        let mut cfg = NetworkConfig::default();
        cfg.quality_offline_threshold = 0.6;

        let peers = Network::new(
            me,
            vec![],
            cfg,
            hopr_db_sql::db::HoprDb::new_in_memory(ChainKeypair::random()).await,
        );

        peers.add(&peer, PeerOrigin::IncomingConnection, vec![]).await.unwrap();

        assert_eq!(
            peers
                .update(&peer, Ok(std::time::Duration::from_millis(13u64)), None)
                .await
                .expect("no error should occur"),
            Some(NetworkTriggeredEvent::UpdateQuality(peer.clone(), 0.1))
        );
        assert_eq!(
            peers.update(&peer, Err(()), None).await.expect("no error should occur"),
            Some(NetworkTriggeredEvent::CloseConnection(peer))
        );

        assert!(peers.has(&public).await);
    }

    #[async_std::test]
    async fn test_network_should_be_healthy_when_a_public_peer_is_pingable_with_high_quality_and_i_am_public() {
        let me: PeerId = OffchainKeypair::random().public().into();
        let peer: PeerId = OffchainKeypair::random().public().into();

        let mut cfg = NetworkConfig::default();
        cfg.quality_offline_threshold = 0.3;

        let peers = Network::new(
            me,
            vec![],
            cfg,
            hopr_db_sql::db::HoprDb::new_in_memory(ChainKeypair::random()).await,
        );

        peers.add(&peer, PeerOrigin::IncomingConnection, vec![]).await.unwrap();

        for _ in 0..3 {
            peers
                .update(&peer, Ok(current_time().as_unix_timestamp()), None)
                .await
                .expect("no error should occur");
        }

        assert_eq!(peers.health().await, Health::Green);
    }

    #[async_std::test]
    async fn test_network_should_be_healthy_when_a_public_peer_is_pingable_with_high_quality_and_another_high_quality_non_public(
    ) {
        let peer: PeerId = OffchainKeypair::random().public().into();
        let peer2: PeerId = OffchainKeypair::random().public().into();

        let mut cfg = NetworkConfig::default();
        cfg.quality_offline_threshold = 0.3;

        let peers = Network::new(
            OffchainKeypair::random().public().into(),
            vec![],
            cfg,
            hopr_db_sql::db::HoprDb::new_in_memory(ChainKeypair::random()).await,
        );

        peers.add(&peer, PeerOrigin::IncomingConnection, vec![]).await.unwrap();
        peers.add(&peer2, PeerOrigin::IncomingConnection, vec![]).await.unwrap();

        for _ in 0..3 {
            peers
                .update(&peer2, Ok(current_time().as_unix_timestamp()), None)
                .await
                .expect("no error should occur");
            peers
                .update(&peer, Ok(current_time().as_unix_timestamp()), None)
                .await
                .expect("no error should occur");
        }

        assert_eq!(peers.health().await, Health::Green);
    }
}
