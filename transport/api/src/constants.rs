use hopr_transport_session::initiation::StartChallenge;
use std::time::Duration;

/// The maximum waiting time for a message send to produce a half-key challenge reply
pub const PACKET_QUEUE_TIMEOUT_MILLISECONDS: std::time::Duration = std::time::Duration::from_millis(15000);

/// The maximum queue size for the network update events
pub(crate) const MAXIMUM_NETWORK_UPDATE_EVENT_QUEUE_SIZE: usize = 4000;

pub const RESERVED_TAG_UPPER_LIMIT: u16 = RESERVED_SESSION_TAG_UPPER_LIMIT;

/// The upper limit value for the session reserved tag range.
///
/// The reserved tags are from range <[`RESERVED_SUBPROTOCOL_TAG_UPPER_LIMIT`], [`RESERVED_SESSION_TAG_UPPER_LIMIT`]) and are
/// specifically dedicated for the internal use of the protocol.
pub(crate) const RESERVED_SESSION_TAG_UPPER_LIMIT: u16 = 1024;

/// The upper limit value for subprotocol reserved tag range.
///
/// The reserved tags are from range <0,[`RESERVED_SUBPROTOCOL_TAG_UPPER_LIMIT`]) and are
/// specifically dedicated for the internal use by the subprotocols.
pub(crate) const RESERVED_SUBPROTOCOL_TAG_UPPER_LIMIT: u16 = 16;

/// Time within Start protocol must finish session initiation.
pub(crate) const SESSION_INITIATION_TIMEOUT: Duration = Duration::from_secs(60);

/// Maximum lifetime of an idle session.
pub(crate) const SESSION_LIFETIME: Duration = Duration::from_secs(2 * 60);

/// The first challenge value used in Start protocol to initiate a session.
pub(crate) const MIN_CHALLENGE: StartChallenge = 1;
