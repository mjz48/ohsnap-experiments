use std::convert::From;

#[derive(Debug)]
pub struct LoggerInitFailedError(pub String);

impl std::fmt::Display for LoggerInitFailedError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Could not initialize logger: {}", self.0)
    }
}

#[derive(Debug)]
pub struct InvalidMQTTPacketError(pub String);

impl std::fmt::Display for InvalidMQTTPacketError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Received invalid packet {}", self.0)
    }
}

#[derive(Debug)]
pub enum MQTTError {
    ConnectHandshakeFailed(String),
    EncodeFailed(String),
    InvalidPacket(InvalidMQTTPacketError),
    LoggerInit(LoggerInitFailedError),
    PacketSendFailed(String),
    SecondConnectReceived(String),
}

impl From<InvalidMQTTPacketError> for MQTTError {
    fn from(e: InvalidMQTTPacketError) -> Self {
        MQTTError::InvalidPacket(e)
    }
}

impl From<LoggerInitFailedError> for MQTTError {
    fn from(e: LoggerInitFailedError) -> Self {
        MQTTError::LoggerInit(e)
    }
}
