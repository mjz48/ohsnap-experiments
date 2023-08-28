#[derive(Debug)]
pub enum Error {
    ConnectHandshakeFailed(String),
    CreateClientTaskFailed(String),
    EncodeFailed(String),
    InvalidPacket(String),
    LoggerInitFailed(String),
    PacketSendFailed(String),
    PacketReceiveFailed(String),
    PublishFailed(String),
    SecondConnectReceived(String),
}

pub type Result<T> = std::result::Result<T, Error>;
