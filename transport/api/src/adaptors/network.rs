use core_network::{
    network::{NetworkEvent, NetworkExternalActions},
    PeerId,
};
use futures::channel::mpsc::Sender;
use tracing::error;

/// Implementation of the network interface allowing emitting and querying
/// the swarm based p2p transport mechanism from the [crate::Network].
#[derive(Debug, Clone)]
pub struct ExternalNetworkInteractions {
    emitter: Sender<NetworkEvent>,
}

impl ExternalNetworkInteractions {
    pub fn new(emitter: Sender<NetworkEvent>) -> Self {
        Self { emitter }
    }
}

impl NetworkExternalActions for ExternalNetworkInteractions {
    fn is_public(&self, _: &PeerId) -> bool {
        // NOTE: In the 2.* releases all nodes are public
        true
    }

    fn emit(&self, event: NetworkEvent) {
        if let Err(e) = self.emitter.clone().start_send(event.clone()) {
            error!("Failed to emit a network status: {}: {}", event, e)
        }
    }
}
