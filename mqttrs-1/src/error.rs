use std::convert::From;

#[derive(Debug)]
pub struct InvalidMQTTPacketError(pub String);

impl std::fmt::Display for InvalidMQTTPacketError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Received invalid packet {}", self.0)
    }
}

#[derive(Debug)]
pub enum MQTTError {
    InvalidPacket(InvalidMQTTPacketError),
}

impl From<InvalidMQTTPacketError> for MQTTError {
    fn from(e: InvalidMQTTPacketError) -> Self {
        MQTTError::InvalidPacket(e)
    }
}
