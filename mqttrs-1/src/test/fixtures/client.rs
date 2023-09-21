use crate::{
    error::{Error, Result},
    mqtt::{self, Packet},
};
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio_util::codec::{BytesCodec, Framed};

pub type FramedBytes = Framed<TcpStream, BytesCodec>;

pub struct Client {
    addr: SocketAddr,
    framed: FramedBytes,
}

impl Client {
    pub async fn new(addr: SocketAddr) -> Result<Client> {
        Ok(Client {
            addr,
            framed: open_framed(addr).await?,
        })
    }

    pub fn get_addr(&self) -> &SocketAddr {
        &self.addr
    }

    pub fn get_framed_mut(&mut self) -> &mut FramedBytes {
        &mut self.framed
    }

    pub async fn send_packet(&mut self, pkt: &Packet) -> Result<()> {
        mqtt::send(pkt, &mut self.framed).await
    }
}

pub async fn open_tcp_connection(addr: SocketAddr) -> Result<TcpStream> {
    TcpStream::connect(addr)
        .await
        .or_else(|e| Err(Error::TokioErr(e)))
}

pub async fn open_framed(addr: SocketAddr) -> Result<FramedBytes> {
    let stream = open_tcp_connection(addr).await?;
    Ok(Framed::new(stream, BytesCodec::new()))
}
