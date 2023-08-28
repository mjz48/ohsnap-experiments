#[derive(Debug)]
pub enum Error {
    ConnectHandshakeFailed(String),
    EncodeFailed(String),
    InvalidPacket(String),
    LoggerInitFailed(String),
    PacketSendFailed(String),
    PacketReceiveFailed(String),
    SecondConnectReceived(String),
}

pub type Result<T> = std::result::Result<T, Error>;
