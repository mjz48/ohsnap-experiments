use tokio::io::Error as TokioError;

#[derive(Debug)]
pub enum Error {
    BrokerMsgSendFailure(String),
    ConnectHandshakeFailed(String),
    CreateClientTaskFailed(String),
    EncodeFailed(String),
    InvalidPacket(String),
    IllegalPacketFromClient(String),
    LoggerInitFailed(String),
    PacketSendFailed(String),
    PacketReceiveFailed(String),
    PublishFailed(String),
    SecondConnectReceived(String),
    TokioErr(TokioError),
}

pub type Result<T> = std::result::Result<T, Error>;
