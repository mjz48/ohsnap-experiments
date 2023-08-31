use crate::broker::BrokerMsg;
use crate::error::{Error, Result};
use bytes::{Bytes, BytesMut};
use futures::{SinkExt, StreamExt};
use log::{info, trace, warn};
use mqttrs::{decode_slice, encode_slice, Packet};
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
    client_tx: Sender<BrokerMsg>,
}

impl std::fmt::Display for ClientHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if let ClientState::Connected(ref session) = self.state {
            write!(f, "{}@{}:{}", session.id, self.addr.ip(), self.addr.port())
        } else {
            write!(f, "{}:{}", self.addr.ip(), self.addr.port())
        }
    }
}

impl ClientHandler {
    pub async fn run(stream: TcpStream, broker_tx: Sender<BrokerMsg>) -> Result<()> {
        let addr = stream.peer_addr().or_else(|e| {
            Err(Error::CreateClientTaskFailed(format!(
                "Could not create client task: {:?}",
                e
            )))
        })?;

        let (client_tx, mut client_rx) = mpsc::channel(BROKER_MSG_CAPACITY);

        let mut client = ClientHandler {
            framed: Framed::new(stream, BytesCodec::new()),
            addr,
            state: ClientState::Initialized,
            broker_tx,
            client_tx,
        };

        match loop {
            tokio::select! {
                // awaiting on message from client's tcp connection
                msg_from_client = client.framed.next() => {
                    match msg_from_client {
                        Some(Ok(bytes)) => {
                            match client.decode_packet(&bytes).await {
                                Ok(Some(pkt)) => {
                                    if let Err(err) = client.update_state(&pkt).await {
                                        break Err(err);
                                    }
                                    if let Err(err) = client.handle_packet(&pkt).await {
                                        break Err(err);
                                    }
                                }
                                Ok(None) => continue,
                                Err(err) => {
                                    break Err(err);
                                }
                            }
                        },
                        None => {
                            info!("Client {} has disconnected.", client);
                            if let ClientState::Connected(ref state) = client.state {
                                client
                                    .broker_tx
                                    .send(BrokerMsg::ClientDisconnected {
                                        client: state.id.clone(),
                                    })
                                    .await
                                    .or_else(|e| Err(Error::BrokerMsgSendFailure(format!("{:?}", e))))?;
                            }

                            break Ok(());
                        },
                        Some(Err(e)) => {
                            break Err(Error::PacketReceiveFailed(format!(
                                "Packet receive failed: {:?}",
                                e
                            )))
                        }
                    }
                },
                // waiting for messages from shared broker state
                msg_from_broker = client_rx.recv() => {
                    match msg_from_broker {
                        Some(BrokerMsg::Publish { dup, retain, ref topic_name, ref payload, .. }) => {
                            let publish = Packet::Publish(mqttrs::Publish {
                                dup,
                                qospid: mqttrs::QosPid::AtMostOnce, // TODO: implement
                                retain,
                                topic_name: topic_name,
                                payload: payload,
                            });

                            let buf_sz = if let mqttrs::Packet::Publish(ref publish) = publish {
                                std::mem::size_of::<mqttrs::Publish>()
                                    + std::mem::size_of::<u8>() * publish.payload.len()
                            } else {
                                0
                            };
                            let mut buf = vec![0u8; buf_sz];
                            if let Err(err) = encode_slice(&publish, &mut buf) {
                                break Err(Error::EncodeFailed(format!("Unable to encode mqttrs packet: {:?}", err)));
                            };

                            if let Err(err) = client.framed.send(Bytes::from(buf)).await {
                                break Err(Error::PacketSendFailed(format!("Unable to send packet: {:?}", err)));
                            };
                        },
                        Some(_) => {
                            trace!("Ignoring unhandled message from shared broker state: {:?}", msg_from_broker);
                        },
                        _ => {
                            // so this should happen if the broker gets dropped before
                            // client_handlers. Should be fine to ignore.
                            break Ok(())
                        }
                    }
                },
            }
        } {
            Err(err) => {
                // there are some situations where we need to tell shared broker
                // state to remove client session info if needed.
                if let ClientState::Connected(ref state) = client.state {
                    client
                        .broker_tx
                        .send(BrokerMsg::ClientDisconnected {
                            client: state.id.clone(),
                        })
                        .await
                        .or_else(|e| Err(Error::BrokerMsgSendFailure(format!("{:?}", e))))?;
                }
                Err(err)
            }
            res => res,
        }
    }

    async fn decode_packet<'a>(&mut self, buf: &'a BytesMut) -> Result<Option<Packet<'a>>> {
        match decode_slice(buf as &[u8]) {
            Ok(res) => Ok(res),
            Err(err) => Err(Error::InvalidPacket(format!(
                "Unable to decode packet: {:?}",
                err
            ))),
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

                    // send init to broker shared state
                    self.broker_tx
                        .send(BrokerMsg::ClientConnected {
                            client: info.id.clone(),
                            client_tx: self.client_tx.clone(),
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
                Err(Error::ConnectHandshakeFailed(format!(
                    "Initialized client expected Connect packet. Received {:?} instead.",
                    pkt
                )))
            }
            ClientState::Connected(_) => {
                if pkt.get_type() == mqttrs::PacketType::Connect {
                    // client has sent a second Connect packet. This is not
                    // allowed. The broker should close connection immediately.
                    return Err(Error::SecondConnectReceived(
                        format!(
                            "Connect packet received from already connected client: {:?} Closing connection.",
                            pkt
                        )
                    ));
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
        trace!("Received Connect packet from client {}.", self);

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
            "Received Connack packet from client {}. This is not allowed. Closing connection: {:?}",
            self, connack
        )))
    }

    async fn handle_publish(&mut self, publish: &mqttrs::Publish<'_>) -> Result<()> {
        trace!("Received Publish packet from client {}.", self);

        let client_id = if let ClientState::Connected(ref info) = self.state {
            info.id.clone()
        } else {
            return Err(Error::ClientHandlerInvalidState(format!(
                "ClientHandler {:?} received published while not connected: {:?}",
                self.addr, self.state
            )));
        };

        self.broker_tx
            .send(BrokerMsg::Publish {
                client: client_id,
                dup: publish.dup,
                retain: publish.retain,
                topic_name: String::from(publish.topic_name),
                payload: publish.payload.to_vec(),
            })
            .await
            .or_else(|e| Err(Error::BrokerMsgSendFailure(format!("{:?}", e))))?;

        Ok(())
    }

    async fn handle_puback(&mut self, _pid: &mqttrs::Pid) -> Result<()> {
        // TODO: implement me
        trace!("Received Puback packet from client {}.", self);
        Ok(())
    }

    async fn handle_pubrec(&mut self, _pid: &mqttrs::Pid) -> Result<()> {
        // TODO: implement me
        trace!("Received Pubrec packet from client {}.", self);
        Ok(())
    }

    async fn handle_pubrel(&mut self, _pid: &mqttrs::Pid) -> Result<()> {
        // TODO: implement me
        trace!("Received Pubrel packet from client {}.", self);
        Ok(())
    }

    async fn handle_pubcomp(&mut self, _pid: &mqttrs::Pid) -> Result<()> {
        // TODO: implement me
        trace!("Received Pubcomp packet from client {}.", self);
        Ok(())
    }

    async fn handle_subscribe(&mut self, subscribe: &mqttrs::Subscribe) -> Result<()> {
        trace!("Received Subscribe packet from client {}.", self);

        if subscribe.topics.len() == 0 {
            warn!("Received subscribe packet with no topics. Ignoring...");
            return Ok(());
        }

        let client_id = if let ClientState::Connected(ref info) = self.state {
            info.id.clone()
        } else {
            return Err(Error::ClientHandlerInvalidState(format!(
                "ClientHandler {:?} received published while not connected: {:?}",
                self.addr, self.state
            )));
        };

        self.broker_tx
            .send(BrokerMsg::Subscribe {
                client: client_id,
                topics: subscribe
                    .topics
                    .iter()
                    .map(|ref subscribe_topic| subscribe_topic.topic_path.clone())
                    .collect(),
            })
            .await
            .or_else(|e| {
                Err(Error::BrokerMsgSendFailure(format!(
                    "Could not send BrokerMsg: {:?}",
                    e
                )))
            })?;

        Ok(())
    }

    async fn handle_suback(&mut self, suback: &mqttrs::Suback) -> Result<()> {
        Err(Error::IllegalPacketFromClient(format!(
            "Received Suback packet from client {}. This is not allowed. Closing connection: {:?}",
            self, suback
        )))
    }

    async fn handle_unsubscribe(&mut self, unsubscribe: &mqttrs::Unsubscribe) -> Result<()> {
        // TODO: implement me
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

    async fn handle_unsuback(&mut self, pid: &mqttrs::Pid) -> Result<()> {
        Err(Error::IllegalPacketFromClient(format!(
            "Received Unsuback packet from client {}. This is not allowed. Closing connection: {:?}",
            self, pid
        )))
    }

    async fn handle_pingreq(&mut self) -> Result<()> {
        trace!("Received pingreq packet from client {}.", self);

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
        trace!("Received pingresp packet from client {}.", self);

        // the MQTT spec doesn't specify that servers can ping clients and that
        // clients have to respond. Technically, it should be okay to receive
        // these packets, but disallow in this implementation for security
        // purposes.
        Err(Error::IllegalPacketFromClient(format!(
            "Received Pingresp packet from client {}. This is not allowed. Closing connection.",
            self
        )))
    }

    async fn handle_disconnect(&mut self) -> Result<()> {
        trace!("Received disconnect packet from client {}.", self);

        // if client is not connected, there is no session info, so we should
        // be fine doing nothing. This ClientHandler task will exit and everything
        // will be wrapped up.
        if let ClientState::Connected(ref session) = self.state {
            self.broker_tx
                .send(BrokerMsg::ClientDisconnected {
                    client: session.id.clone(),
                })
                .await
                .or_else(|e| Err(Error::BrokerMsgSendFailure(format!("{:?}", e))))?;
        }

        Ok(())
    }
}
