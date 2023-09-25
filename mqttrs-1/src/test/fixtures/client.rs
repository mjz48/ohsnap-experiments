use crate::{
    error::{Error, Result},
    mqtt::{self, Packet},
};
use futures::StreamExt;
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

    pub async fn send(&mut self, pkt: &Packet) -> Result<()> {
        mqtt::send(pkt, &mut self.framed).await
    }

    pub async fn receive(&mut self) -> Result<Option<Packet>> {
        match self.framed.next().await {
            Some(res) => {
                let buf = res.or_else(|e| {
                    Err(Error::PacketReceiveFailed(format!(
                        "Packet receive failed: {:?}",
                        e
                    )))
                })?;

                Ok(mqtt::decode(&buf)?)
            }
            None => Ok(None),
        }
    }

    pub async fn expect_connect(&mut self) -> Result<mqtt::Connect> {
        match self.receive().await? {
            Some(pkt) => {
                if let Packet::Connect(connect) = pkt {
                    Ok(connect)
                } else {
                    Err(Error::PacketReceiveFailed(format!(
                        "Wrong packet received. Expected Packet::Connect, got {:?}",
                        pkt
                    )))
                }
            }
            None => Err(Error::PacketReceiveFailed(
                "Did not receive valid packet.".into(),
            )),
        }
    }

    pub async fn expect_connack(&mut self) -> Result<mqtt::Connack> {
        match self.receive().await? {
            Some(pkt) => {
                if let Packet::Connack(connack) = pkt {
                    Ok(connack)
                } else {
                    Err(Error::PacketReceiveFailed(format!(
                        "Wrong packet received. Expected Packet::Connect, got {:?}",
                        pkt
                    )))
                }
            }
            None => Err(Error::PacketReceiveFailed(
                "Did not receive valid packet.".into(),
            )),
        }
    }

    pub async fn expect_publish(&mut self) -> Result<mqtt::Publish> {
        match self.receive().await? {
            Some(pkt) => {
                if let Packet::Publish(publish) = pkt {
                    Ok(publish)
                } else {
                    Err(Error::PacketReceiveFailed(format!(
                        "Wrong packet received. Expected Packet::Connect, got {:?}",
                        pkt
                    )))
                }
            }
            None => Err(Error::PacketReceiveFailed(
                "Did not receive valid packet.".into(),
            )),
        }
    }

    pub async fn expect_puback(&mut self) -> Result<mqtt::Pid> {
        match self.receive().await? {
            Some(pkt) => {
                if let Packet::Puback(pid) = pkt {
                    Ok(pid)
                } else {
                    Err(Error::PacketReceiveFailed(format!(
                        "Wrong packet received. Expected Packet::Puback, got {:?}",
                        pkt
                    )))
                }
            }
            None => Err(Error::PacketReceiveFailed(
                "Did not receive valid packet.".into(),
            )),
        }
    }

    pub async fn expect_pubrec(&mut self) -> Result<mqtt::Pid> {
        match self.receive().await? {
            Some(pkt) => {
                if let Packet::Pubrec(pid) = pkt {
                    Ok(pid)
                } else {
                    Err(Error::PacketReceiveFailed(format!(
                        "Wrong packet received. Expected Packet::Pubrec, got {:?}",
                        pkt
                    )))
                }
            }
            None => Err(Error::PacketReceiveFailed(
                "Did not receive valid packet.".into(),
            )),
        }
    }

    pub async fn expect_pubrel(&mut self) -> Result<mqtt::Pid> {
        match self.receive().await? {
            Some(pkt) => {
                if let Packet::Pubrel(pid) = pkt {
                    Ok(pid)
                } else {
                    Err(Error::PacketReceiveFailed(format!(
                        "Wrong packet received. Expected Packet::Pubrel, got {:?}",
                        pkt
                    )))
                }
            }
            None => Err(Error::PacketReceiveFailed(
                "Did not receive valid packet.".into(),
            )),
        }
    }

    pub async fn expect_pubcomp(&mut self) -> Result<mqtt::Pid> {
        match self.receive().await? {
            Some(pkt) => {
                if let Packet::Pubcomp(pid) = pkt {
                    Ok(pid)
                } else {
                    Err(Error::PacketReceiveFailed(format!(
                        "Wrong packet received. Expected Packet::Pubcomp, got {:?}",
                        pkt
                    )))
                }
            }
            None => Err(Error::PacketReceiveFailed(
                "Did not receive valid packet.".into(),
            )),
        }
    }

    pub async fn expect_subscribe(&mut self) -> Result<mqtt::Subscribe> {
        match self.receive().await? {
            Some(pkt) => {
                if let Packet::Subscribe(subscribe) = pkt {
                    Ok(subscribe)
                } else {
                    Err(Error::PacketReceiveFailed(format!(
                        "Wrong packet received. Expected Packet::Subscribe, got {:?}",
                        pkt
                    )))
                }
            }
            None => Err(Error::PacketReceiveFailed(
                "Did not receive valid packet.".into(),
            )),
        }
    }

    pub async fn expect_suback(&mut self) -> Result<mqtt::Suback> {
        match self.receive().await? {
            Some(pkt) => {
                if let Packet::Suback(suback) = pkt {
                    Ok(suback)
                } else {
                    Err(Error::PacketReceiveFailed(format!(
                        "Wrong packet received. Expected Packet::Suback, got {:?}",
                        pkt
                    )))
                }
            }
            None => Err(Error::PacketReceiveFailed(
                "Did not receive valid packet.".into(),
            )),
        }
    }

    pub async fn expect_unsubscribe(&mut self) -> Result<mqtt::Unsubscribe> {
        match self.receive().await? {
            Some(pkt) => {
                if let Packet::Unsubscribe(unsubscribe) = pkt {
                    Ok(unsubscribe)
                } else {
                    Err(Error::PacketReceiveFailed(format!(
                        "Wrong packet received. Expected Packet::Unsubscribe, got {:?}",
                        pkt
                    )))
                }
            }
            None => Err(Error::PacketReceiveFailed(
                "Did not receive valid packet.".into(),
            )),
        }
    }

    pub async fn expect_unsuback(&mut self) -> Result<mqtt::Pid> {
        match self.receive().await? {
            Some(pkt) => {
                if let Packet::Unsuback(pid) = pkt {
                    Ok(pid)
                } else {
                    Err(Error::PacketReceiveFailed(format!(
                        "Wrong packet received. Expected Packet::Unsuback, got {:?}",
                        pkt
                    )))
                }
            }
            None => Err(Error::PacketReceiveFailed(
                "Did not receive valid packet.".into(),
            )),
        }
    }

    pub async fn expect_pingreq(&mut self) -> Result<()> {
        match self.receive().await? {
            Some(pkt) => {
                if let Packet::Pingreq = pkt {
                    Ok(())
                } else {
                    Err(Error::PacketReceiveFailed(format!(
                        "Wrong packet received. Expected Packet::Pingreq, got {:?}",
                        pkt
                    )))
                }
            }
            None => Err(Error::PacketReceiveFailed(
                "Did not receive valid packet.".into(),
            )),
        }
    }

    pub async fn expect_pingresp(&mut self) -> Result<()> {
        match self.receive().await? {
            Some(pkt) => {
                if let Packet::Pingresp = pkt {
                    Ok(())
                } else {
                    Err(Error::PacketReceiveFailed(format!(
                        "Wrong packet received. Expected Packet::Pingresp, got {:?}",
                        pkt
                    )))
                }
            }
            None => Err(Error::PacketReceiveFailed(
                "Did not receive valid packet.".into(),
            )),
        }
    }

    pub async fn expect_disconnect(&mut self) -> Result<()> {
        match self.receive().await? {
            Some(pkt) => {
                if let Packet::Disconnect = pkt {
                    Ok(())
                } else {
                    Err(Error::PacketReceiveFailed(format!(
                        "Wrong packet received. Expected Packet::Disconnect, got {:?}",
                        pkt
                    )))
                }
            }
            None => Err(Error::PacketReceiveFailed(
                "Did not receive valid packet.".into(),
            )),
        }
    }

    pub async fn expect_stream_closed(&mut self) -> Result<()> {
        match self.framed.next().await {
            None => Ok(()),
            Some(msg) => {
                msg.or_else(|e| {
                    Err(Error::PacketReceiveFailed(format!(
                        "Packet receive failed: {:?}",
                        e
                    )))
                })?;

                Err(Error::PacketReceiveFailed(
                    "Expected stream closed, but stream is still open.".into(),
                ))
            }
        }
    }
}

async fn open_tcp_connection(addr: SocketAddr) -> Result<TcpStream> {
    TcpStream::connect(addr)
        .await
        .or_else(|e| Err(Error::TokioErr(e)))
}

async fn open_framed(addr: SocketAddr) -> Result<FramedBytes> {
    let stream = open_tcp_connection(addr).await?;
    Ok(Framed::new(stream, BytesCodec::new()))
}
