use crate::broker::BrokerMsg;
use crate::error::{Error, Result};
use bytes::{Bytes, BytesMut};
use futures::{SinkExt, StreamExt};
use log::{error, info, trace, warn};
use mqttrs::{decode_slice, encode_slice, Packet};
use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{self, Sender};
use tokio_util::codec::{BytesCodec, Framed};

const BROKER_MSG_CAPACITY: usize = 100;

#[derive(Debug, Eq, PartialEq)]
pub struct ClientSession {
    pub id: String,
}

#[derive(Debug, Eq, PartialEq)]
pub enum ClientState {
    Initialized,
    Connected(ClientSession),
}

pub struct ClientHandler {
    framed: Framed<TcpStream, BytesCodec>,
    addr: SocketAddr,
    state: ClientState,
    broker_tx: Sender<BrokerMsg>,
}

impl ClientHandler {
    pub async fn run(stream: TcpStream, broker_tx: Sender<BrokerMsg>) -> Result<()> {
        let addr = stream.peer_addr().or_else(|e| {
            Err(Error::CreateClientTaskFailed(format!(
                "Could not create client task: {:?}",
                e
            )))
        })?;

        let mut client = ClientHandler {
            framed: Framed::new(stream, BytesCodec::new()),
            addr,
            state: ClientState::Initialized,
            broker_tx,
        };

        loop {
            let bytes = match client.framed.next().await {
                Some(Ok(bytes)) => bytes,
                None => {
                    if let ClientState::Connected(ref state) = client.state {
                        info!("Client '{}@{}' has disconnected.", state.id, client.addr);
                    } else {
                        info!("Client has disconnected.");
                    }

                    break Ok(());
                }
                Some(Err(e)) => {
                    let err = Error::PacketReceiveFailed(format!("Packet receive failed: {:?}", e));
                    error!("{:?}", err);
                    break Err(err);
                }
            };

            if let Some(pkt) = client.decode_packet(&bytes).await? {
                client.update_state(&pkt).await?;
                client.handle_packet(&pkt).await?;
            };
        }
    }

    async fn decode_packet<'a>(&mut self, buf: &'a BytesMut) -> Result<Option<Packet<'a>>> {
        match decode_slice(buf as &[u8]) {
            Ok(res) => Ok(res),
            Err(err) => {
                let err = Error::InvalidPacket(format!("Unable to decode packet: {:?}", err));
                error!("{:?}", err);
                Err(err)
            }
        }
    }

    async fn update_state<'a>(&mut self, pkt: &Packet<'a>) -> Result<()> {
        match self.state {
            ClientState::Initialized => {
                if let mqttrs::Packet::Connect(connect) = pkt {
                    trace!("Client has sent connect packet: {:?}", pkt);

                    // store client info and session data here (it's probably more logically clean
                    // to do this in handle_connect, but if we do it here we don't have to make
                    // ClientInfo an option.)
                    let info = ClientSession {
                        id: String::from(connect.client_id),
                    };

                    let (client_tx, _client_rx) = mpsc::channel(BROKER_MSG_CAPACITY);

                    // send init to broker shared state
                    self.broker_tx
                        .send(BrokerMsg::ClientConnected {
                            client: info.id.clone(),
                            client_tx,
                        })
                        .await
                        .or_else(|e| {
                            Err(Error::BrokerMsgSendFailure(format!(
                                "Could not send BrokerMsg: {:?}",
                                e
                            )))
                        })?;

                    info!("Client '{}@{}' connected.", info.id, self.addr);

                    self.state = ClientState::Connected(info);
                    return Ok(());
                }

                // if we fall through here, client has open a TCP connection
                // but sent a packet that wasn't Connect. This is not allowed,
                // the broker should close the connection.
                let err = Error::ConnectHandshakeFailed(format!(
                    "Initialized client expected Connect packet. Received {:?} instead.",
                    pkt
                ));
                error!("{:?}", err);

                Err(err)
            }
            ClientState::Connected(_) => {
                if pkt.get_type() == mqttrs::PacketType::Connect {
                    // client has sent a second Connect packet. This is not
                    // allowed. The broker should close connection immediately.
                    let err = Error::SecondConnectReceived(
                        format!(
                            "Connect packet received from already connected client: {:?} Closing connection.",
                            pkt
                        )
                    );
                    error!("{:?}", err);
                    return Err(err);
                }

                Ok(())
            }
        }
    }

    async fn handle_packet<'a>(&mut self, pkt: &Packet<'a>) -> Result<()> {
        match pkt {
            Packet::Connect(connect) => self.handle_connect(connect).await?,
            Packet::Connack(connack) => self.handle_connack(connack).await?,
            Packet::Publish(publish) => self.handle_publish(publish).await?,
            Packet::Puback(pid) => self.handle_puback(pid).await?,
            Packet::Pubrec(pid) => self.handle_pubrec(pid).await?,
            Packet::Pubrel(pid) => self.handle_pubrel(pid).await?,
            Packet::Pubcomp(pid) => self.handle_pubcomp(pid).await?,
            Packet::Subscribe(subscribe) => self.handle_subscribe(subscribe).await?,
            Packet::Suback(suback) => self.handle_suback(suback).await?,
            Packet::Unsubscribe(unsubscribe) => self.handle_unsubscribe(unsubscribe).await?,
            Packet::Unsuback(pid) => self.handle_unsuback(pid).await?,
            Packet::Pingreq => self.handle_pingreq().await?,
            Packet::Pingresp => self.handle_pingresp().await?,
            Packet::Disconnect => self.handle_disconnect().await?,
        }

        Ok(())
    }

    async fn handle_connect(&mut self, _connect: &mqttrs::Connect<'_>) -> Result<()> {
        let connack = Packet::Connack(mqttrs::Connack {
            session_present: false,                    // TODO: implement session handling
            code: mqttrs::ConnectReturnCode::Accepted, // TODO: implement connection error handling
        });
        let mut buf = vec![0u8; 8];

        encode_slice(&connack, &mut buf as &mut [u8]).or_else(|e| {
            Err(Error::EncodeFailed(format!(
                "Unable to encode packet: {:?}",
                e
            )))
        })?;
        trace!("CONNACK: {:#?}", connack);

        Ok(self.framed.send(Bytes::from(buf)).await.or_else(|e| {
            Err(Error::PacketSendFailed(format!(
                "Unable to send packet: {:?}",
                e
            )))
        })?)
    }

    async fn handle_connack(&mut self, connack: &mqttrs::Connack) -> Result<()> {
        Err(Error::IllegalPacketFromClient(format!(
            "Received Connack packet from Client. This is not allowed: {:?}",
            connack
        )))
    }

    async fn handle_publish(&mut self, publish: &mqttrs::Publish<'_>) -> Result<()> {
        let payload_str = if let Ok(s) = String::from_utf8(publish.payload.to_vec()) {
            s
        } else {
            let err =
                Error::PublishFailed("Could not convert publish packet payload to string.".into());
            error!("{:?}", err);

            return Err(err);
        };
        let topic_str = publish.topic_name.to_owned();

        info!("Received publish for topic '{}'...", topic_str);
        info!("Message contents: '{}'...", payload_str);

        // TODO: implement message publishing to all subscribed clients.
        Ok(())
    }

    async fn handle_puback(&mut self, _pid: &mqttrs::Pid) -> Result<()> {
        Ok(())
    }

    async fn handle_pubrec(&mut self, _pid: &mqttrs::Pid) -> Result<()> {
        Ok(())
    }

    async fn handle_pubrel(&mut self, _pid: &mqttrs::Pid) -> Result<()> {
        Ok(())
    }

    async fn handle_pubcomp(&mut self, _pid: &mqttrs::Pid) -> Result<()> {
        Ok(())
    }

    async fn handle_subscribe(&mut self, subscribe: &mqttrs::Subscribe) -> Result<()> {
        let num_topics = subscribe.topics.len();
        if num_topics == 0 {
            warn!("Received subscribe packet that does not have any topics. Ignoring...");
        }

        info!("Received subscription request for the following topics:\n");
        for topic in subscribe.topics.iter() {
            info!("  {}", topic.topic_path);
        }
        info!("\n");

        // TODO: the rest of this function sends a dummy message to a random
        // topic followed by subscribing client. This is only for testing
        // purposes.

        for _ in 0..10 {
            std::thread::sleep(std::time::Duration::from_secs(5));

            let mut rng = StdRng::from_entropy();
            let rand_idx = rng.next_u32() as usize;
            let topic = &subscribe.topics[rand_idx % num_topics].topic_path;

            info!("Publishing dummy message to {}...", topic);

            let pkt = Packet::Publish(mqttrs::Publish {
                dup: false,
                qospid: mqttrs::QosPid::AtMostOnce,
                retain: false,
                topic_name: topic,
                payload: "hello from broker".as_bytes(),
            });
            drop(rng);

            let buf_sz = if let Packet::Publish(ref publish) = pkt {
                std::mem::size_of::<mqttrs::Publish>()
                    + std::mem::size_of::<u8>() * publish.payload.len()
            } else {
                0
            };
            let mut buf = vec![0u8; buf_sz];

            let encoded = encode_slice(&pkt, &mut buf);
            assert!(encoded.is_ok());

            let encoded = Bytes::from(buf);
            let _res = self.framed.send(encoded).await;
        }

        Ok(())
    }

    async fn handle_suback(&mut self, _suback: &mqttrs::Suback) -> Result<()> {
        Ok(())
    }

    async fn handle_unsubscribe(&mut self, unsubscribe: &mqttrs::Unsubscribe) -> Result<()> {
        info!("Received unsubscribe request for the following topics:\n");
        for ref topic in unsubscribe.topics.iter() {
            info!("  {}", topic);
        }
        info!("\n");

        // send UNSUBACK
        // TODO: implement pid handling
        let pkt = Packet::Unsuback(mqttrs::Pid::new());
        let mut buf = vec![0u8; std::mem::size_of::<Packet>() + std::mem::size_of::<mqttrs::Pid>()];

        let encoded = encode_slice(&pkt, &mut buf);
        assert!(encoded.is_ok());

        let encoded = Bytes::from(buf);
        let _res = self.framed.send(encoded).await;

        // TODO: implement unsubscribe
        Ok(())
    }

    async fn handle_unsuback(&mut self, _pid: &mqttrs::Pid) -> Result<()> {
        Ok(())
    }

    async fn handle_pingreq(&mut self) -> Result<()> {
        // respond to ping requests; will keep the connection alive
        let ping_resp = Packet::Pingresp {};
        let mut buf = vec![0u8; 3];

        encode_slice(&ping_resp, &mut buf as &mut [u8]).or_else(|e| {
            Err(Error::EncodeFailed(format!(
                "Unable to encode packet: {:?}",
                e
            )))
        })?;

        Ok(self.framed.send(Bytes::from(buf)).await.or_else(|e| {
            Err(Error::PacketSendFailed(format!(
                "Unable to send packet: {:?}",
                e
            )))
        })?)
    }

    async fn handle_pingresp(&mut self) -> Result<()> {
        Ok(())
    }

    async fn handle_disconnect(&mut self) -> Result<()> {
        Ok(())
    }
}
