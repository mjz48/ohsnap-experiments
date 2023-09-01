use tokio::io::Error as TokioError;

#[derive(Debug)]
pub enum Error {
    BrokerMsgSendFailure(String),
    ClientHandlerInvalidState(String),
    CreateClientTaskFailed(String),
    EncodeFailed(String),
    InvalidPacket(String),
    LoggerInitFailed(String),
    MQTTProtocolViolation(String),
    PacketSendFailed(String),
    PacketReceiveFailed(String),
    PublishFailed(String),
    TokioErr(TokioError),
}

pub type Result<T> = std::result::Result<T, Error>;
