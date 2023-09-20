use crate::error::{Error, Result};
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio_util::codec::{BytesCodec, Framed};

pub type FramedBytes = Framed<TcpStream, BytesCodec>;

pub async fn open_tcp_connection(addr: SocketAddr) -> Result<TcpStream> {
    TcpStream::connect(addr)
        .await
        .or_else(|e| Err(Error::TokioErr(e)))
}

pub async fn open_framed(addr: SocketAddr) -> Result<FramedBytes> {
    let stream = open_tcp_connection(addr).await?;
    Ok(Framed::new(stream, BytesCodec::new()))
}
