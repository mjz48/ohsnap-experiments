use tokio::io::Error as TokioError;

/// Custom MQTT Error types defined for broker events
#[derive(Debug)]
pub enum Error {
    /// This occurs if there is a problem with the channels between shared broker state and client
    /// handlers in both directions
    BrokerMsgSendFailure(String),
    /// This is an internal error. Occurs when a client handler task receives a packet that does
    /// not match its internal state. (i.e. received publish packet when not in Connected state)
    ClientHandlerInvalidState(String),
    /// Broker is unable to create a new client handler task
    CreateClientTaskFailed(String),
    /// mqttrs::encode_slice has failed with error
    EncodeFailed(String),
    /// An unknown packet type has been received either from the broker tcp stream or BrokerMsg
    /// channel.
    InvalidPacket(String),
    /// Problem with loggin happened
    LoggerError(String),
    /// An MQTT protocol violation has occurred. Usually receiving an illegal packet type
    MQTTProtocolViolation(String),
    /// Could not send packet to client over tcp connection
    PacketSendFailed(String),
    /// Could not receive a packet from client over tcp connection
    PacketReceiveFailed(String),
    /// Wrapper for Tokio errors
    TokioErr(TokioError),
}

impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        match self {
            Error::TokioErr(_) => {
                if let Error::TokioErr(_) = other {
                    true
                } else {
                    false
                }
            }
            err => err == other,
        }
    }
}
impl Eq for Error {}

pub type Result<T> = std::result::Result<T, Error>;
