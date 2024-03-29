use crate::mqtt::{Packet, Publish, Subscribe, Unsubscribe};
use tokio::sync::mpsc::Sender;

/// Message types sent in both directions between shared broker state and
/// client handler.
///
/// It would be nice to package an mqttrs::Packet inside these messages instead
/// of duplicating all the fields, but the new revision of the library requires
/// a lifetime reference to the underlying BytesMut buffer and that's too
/// inconvenient.
#[derive(Debug, Clone)]
pub enum BrokerMsg {
    /// Sent when a new tcp connection is detected and the client has successfully
    /// authenticated with connection handshake.
    ClientConnected {
        /// client identifier
        client: String,
        /// new channel opened for communication to this client
        client_tx: Sender<BrokerMsg>,
    },
    /// The ClientHandler sends this message to itself when a "reasonable amount
    /// of time" has passed between the client opening a tcp connection, but has
    /// not sent an MQTT connection packet. This does not have a corresponding
    /// MQTT control packet.
    ClientConnectionTimeout,
    /// Sent when a client or broker disconnects intentionally or also when
    /// either disconnects due to protocol violation.
    ClientDisconnected {
        /// client identifier
        client: String,
    },
    /// Sent when a client wants to publish a message. Also sent by broker to
    /// subscribed clients to publish messages. NOTE: except for `client`,
    /// these fields are copied from mqttrs::Publish.
    Publish {
        /// client identifier
        client: String,
        /// publish packet data
        packet: Publish,
    },
    /// Sent when a client wants to subscribe to new topic paths. NOTE: these
    /// fields are copied from mqttrs::Subscribe, except for `client`.
    Subscribe {
        /// client identifier
        client: String,
        /// subscribe packet data
        packet: Subscribe,
    },
    /// Sent when a client wants to unsubscribe to new topic paths. NOTE: these
    /// fields are copied from mqttrs:Unsubscribe, except for `client`.
    Unsubscribe {
        /// client identifier
        client: String,
        /// unsubscribe packet data
        packet: Unsubscribe,
    },
    /// Sent when the client needs to retransmit a packet due to a QoS protocol
    /// timeout occuring. The client handler sends this packet to itself.
    QoSRetry {
        /// client identifier
        client: String,
        /// mqtt control packet to resend to client
        packet: Packet,
    },
}
