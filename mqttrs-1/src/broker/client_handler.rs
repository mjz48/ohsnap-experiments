use crate::broker::session;
use crate::broker::{BrokerMsg, Config, Session};
use crate::error::{Error, Result};
use bytes::{Bytes, BytesMut};
use futures::{SinkExt, StreamExt};
use log::{debug, info, trace, warn};
use mqttrs::{decode_slice, encode_slice, Packet, PacketType, QosPid};
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{self, Sender};
use tokio_util::codec::{BytesCodec, Framed};

/// reserved capacity for broker shared state -> client handler channel
const BROKER_MSG_CAPACITY: usize = 100;

/// Client State machine definition
#[derive(Debug, Eq, PartialEq)]
pub enum ClientState {
    /// Tcp connection has occurred, but connection handshake not done
    Initialized,
    /// Client connection handshake done, ready to send/receive packets
    Connected(Session),
}

/// Contains state and business logic to interface with MQTT clients
pub struct ClientHandler {
    /// copy of broker config
    config: Config,
    /// tokio framed wrapping tcp stream to send/receive MQTT control packets
    framed: Framed<TcpStream, BytesCodec>,
    /// address from broker config for logging purposes
    addr: SocketAddr,
    /// client state machine data
    state: ClientState,
    /// channel to send shared broker state messages
    broker_tx: Sender<BrokerMsg>,
    /// channel sender to shared broker state to communicate with this client handler
    client_tx: Sender<BrokerMsg>,
}

impl std::fmt::Display for ClientHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if let Some(session) = self.get_session() {
            write!(
                f,
                "{}@{}:{}",
                session.id(),
                self.addr.ip(),
                self.addr.port()
            )
        } else {
            write!(f, "{}:{}", self.addr.ip(), self.addr.port())
        }
    }
}

impl ClientHandler {
    /// Create and run client handler function
    ///
    /// # Arguments
    ///
    /// * `stream` - tcp stream with client on other end
    /// * `broker_tx` - channel to communicate with broker shared state
    ///
    /// # Errors
    ///
    /// This function may return the following errors:
    ///
    ///     * BrokerMsgSendFailure
    ///     * CreateClientTaskFailed
    ///     * EncodeFailed
    ///     * MQTTProtocolViolation
    ///     * PacketReceiveFailed
    ///     * InvalidPacket
    ///     * TokioErr
    ///
    pub async fn run(
        config: Config,
        stream: TcpStream,
        broker_tx: Sender<BrokerMsg>,
    ) -> Result<()> {
        let addr = stream.peer_addr().or_else(|e| {
            Err(Error::CreateClientTaskFailed(format!(
                "Could not create client task: {:?}",
                e
            )))
        })?;

        let (client_tx, mut client_rx) = mpsc::channel(BROKER_MSG_CAPACITY);

        let mut client = ClientHandler {
            config,
            framed: Framed::new(stream, BytesCodec::new()),
            addr,
            state: ClientState::Initialized,
            broker_tx,
            client_tx,
        };

        // TODO: if the client opens a TCP connection but doesn't send a connect
        // packet within a "reasonable amount of time", the server SHOULD close
        // the connection. Implement this.

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
                            break client.handle_client_disconnect().await;
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
                        Some(msg) => {
                            if let Err(err) = client.decode_broker_msg(msg).await {
                                break Err(err);
                            }
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
                client.handle_client_disconnect().await?;
                Err(err)
            }
            res => res,
        }
    }

    /// Given a byte array from tcp connection, decode into MQTT packet. this
    /// function will only ever return at most one MQTT packet.
    ///
    /// # Arguments
    ///
    /// * `buf` - reference to byte array possibly containing MQTT packet
    ///
    /// # Errors
    ///
    /// This function may return the following errors:
    ///
    ///     * InvalidPacket
    async fn decode_packet<'a>(&mut self, buf: &'a BytesMut) -> Result<Option<Packet<'a>>> {
        match decode_slice(buf as &[u8]) {
            Ok(res) => Ok(res),
            Err(err) => Err(Error::InvalidPacket(format!(
                "Unable to decode packet: {:?}",
                err
            ))),
        }
    }

    /// Update the internal state of the client. This is useful for keeping
    /// track of which MQTT control packets are allowed at a given time.
    ///
    /// # Arguments
    ///
    /// * `pkt` - MQTT control packet that was just received
    ///
    /// # Errors
    ///
    /// This function may return the following errors:
    ///
    ///     * BrokerMsgSendFailure
    ///     * MQTTProtocolViolation
    ///
    async fn update_state<'a>(&mut self, pkt: &Packet<'a>) -> Result<()> {
        match self.state {
            ClientState::Initialized => {
                if let Packet::Connect(connect) = pkt {
                    trace!("Client has sent connect packet: {:?}", pkt);

                    // TODO: validate connect packet format
                    //   * if the supplied client_id already exists in shared session data,
                    //     the server should send that client a DISCONNECT packet with
                    //     reason code 0x8E (Session taken over) and then close its network
                    //     connection. If the previous client had a will message, it must
                    //     be handled and published as per the spec.
                    //
                    //   * purge session data if necessary according to the Clean Start flag
                    // TODO: implement authentication + authorization support
                    //
                    // TODO: client may start sending packets before receiving this connack.
                    // Need to make sure the server can handle this. If the connection
                    // attempt fails, the server must not process any subsequent packets.
                    // Also, it is suggested to implement backpressure or even close the
                    // connection if the client sends too much data before authentication
                    // is complete (to avoid DDoS attempts).

                    // store client info and session data here (it's probably more logically clean
                    // to do this in handle_connect, but if we do it here we don't have to make
                    // ClientInfo an option.)
                    let info = Session::new(connect.client_id);
                    trace!("Creating new session data: {:?}", info);

                    // send init to broker shared state
                    self.broker_tx
                        .send(BrokerMsg::ClientConnected {
                            client: info.clone(),
                            client_tx: self.client_tx.clone(),
                        })
                        .await
                        .or_else(|e| {
                            Err(Error::BrokerMsgSendFailure(format!(
                                "Could not send BrokerMsg: {:?}",
                                e
                            )))
                        })?;

                    // TODO: keep alive behavior should be started here
                    // TODO: need to retry active transaction packets as part
                    // of QoS > 0 flow

                    info!("Client '{}@{}' connected.", info.id(), self.addr);

                    self.state = ClientState::Connected(info);
                    return Ok(());
                }

                // if we fall through here, client has open a TCP connection
                // but sent a packet that wasn't Connect. This is not allowed,
                // the broker should close the connection.
                Err(Error::MQTTProtocolViolation(format!(
                    "Initialized client expected Connect packet. Received {:?} instead.",
                    pkt
                )))
            }
            ClientState::Connected(_) => {
                if pkt.get_type() == PacketType::Connect {
                    // client has sent a second Connect packet. This is not
                    // allowed. The broker should close connection immediately.
                    return Err(Error::MQTTProtocolViolation(
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

    /// Internal function for updating client handler state when a client
    /// disconnects. Basically all we need to do here is send a message to the
    /// shared broker state notifying that the client has disconnected and
    /// the shared broker state should handle the rest.
    ///
    /// # Errors
    ///
    /// This function may throw the following errors:
    ///
    ///     * BrokerMsgSendFailure
    async fn handle_client_disconnect(&self) -> Result<()> {
        if let ClientState::Connected(ref state) = self.state {
            self.broker_tx
                .send(BrokerMsg::ClientDisconnected {
                    client: state.id().to_string(),
                })
                .await
                .or_else(|e| Err(Error::BrokerMsgSendFailure(format!("{:?}", e))))?;
        }

        Ok(())
    }

    /// Attempt to get the client session data.
    fn get_session(&self) -> Option<&Session> {
        if let ClientState::Connected(ref state) = self.state {
            Some(state)
        } else {
            None
        }
    }

    /// Attempt to get a mutable reference to the client session data
    fn get_session_mut(&mut self) -> Option<&mut Session> {
        if let ClientState::Connected(ref mut state) = self.state {
            Some(state)
        } else {
            None
        }
    }

    /// Perform client handler business logic when a valid MQTT packet is received
    ///
    /// # Arguments
    ///
    /// * `pkt` - MQTT control packet that was just received
    ///
    /// # Errors
    ///
    /// This function may throw the following errors:
    ///
    ///     * BrokerMsgSendFailure
    ///     * ClientHandlerInvalidState
    ///     * CreateClientTaskFailed
    ///     * EncodeFailed
    ///     * InvalidPacket
    ///     * LoggerInitFailed
    ///     * MQTTProtocolViolation
    ///     * PacketSendFailed
    ///     * PacketReceiveFailed
    ///     * TokioErr
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
        // calculate buf size by multiplying Connack size by two. The actual
        // size is slightly larger than Connack size but it is unpredictable
        // and dependent on platform, so use 2x size as a good compromise.
        let buf_sz = std::mem::size_of::<Packet>() + std::mem::size_of::<mqttrs::Connack>();
        let mut buf = vec![0u8; buf_sz];

        encode_slice(&connack, &mut buf as &mut [u8]).or_else(|e| {
            Err(Error::EncodeFailed(format!(
                "Unable to encode packet: {:?}",
                e
            )))
        })?;
        trace!("Sending Connack packet in response: {:?}", connack);

        Ok(self.framed.send(Bytes::from(buf)).await.or_else(|e| {
            Err(Error::PacketSendFailed(format!(
                "Unable to send packet: {:?}",
                e
            )))
        })?)
    }

    async fn handle_connack(&mut self, connack: &mqttrs::Connack) -> Result<()> {
        Err(Error::MQTTProtocolViolation(format!(
            "Received Connack packet from client {}. This is not allowed. Closing connection: {:?}",
            self, connack
        )))
    }

    async fn handle_publish(&mut self, publish: &mqttrs::Publish<'_>) -> Result<()> {
        trace!("Received Publish packet from client {}.", self);

        let session = match self.get_session_mut() {
            Some(session) => session,
            None => {
                return Err(Error::ClientHandlerInvalidState(format!(
                    "ClientHandler {:?} received published while not connected: {:?}",
                    self.addr, self.state
                )));
            }
        };

        let client_id = session.id().to_string();

        match publish.qospid {
            mqttrs::QosPid::AtMostOnce => (), // no follow up required
            mqttrs::QosPid::AtLeastOnce(pid) => {
                // if QoS == 1, need to send PubAck, no need to initialize new
                // transaction since the required transmissions are already done
                let puback = Packet::Puback(pid);
                let buf_sz = std::mem::size_of::<Packet>() + std::mem::size_of::<mqttrs::Pid>();
                let mut buf = vec![0u8; buf_sz];

                encode_slice(&puback, &mut buf as &mut [u8]).or_else(|e| {
                    Err(Error::EncodeFailed(format!(
                        "Unable to encode packet: {:?}",
                        e
                    )))
                })?;
                trace!(
                    "Sending puback response for QoS = {:?}: {:?}",
                    publish.qospid,
                    puback
                );

                self.framed.send(Bytes::from(buf)).await.or_else(|e| {
                    Err(Error::PacketSendFailed(format!(
                        "Unable to send packet: {:?}",
                        e
                    )))
                })?;
            }
            mqttrs::QosPid::ExactlyOnce(pid) => {
                // initialize new transaction
                let txn = session.init_txn(pid, mqttrs::QoS::ExactlyOnce)?;

                let pubrec = Packet::Pubrec(pid);
                let buf_sz = std::mem::size_of::<Packet>() + std::mem::size_of::<mqttrs::Pid>();
                let mut buf = vec![0u8; buf_sz];

                encode_slice(&pubrec, &mut buf as &mut [u8]).or_else(|e| {
                    Err(Error::EncodeFailed(format!(
                        "Unable to encode packet: {:?}",
                        e
                    )))
                })?;
                trace!(
                    "Sending pubrec response for QoS = {:?}: {:?}",
                    publish.qospid,
                    pubrec
                );

                // update txn from publish -> pubrec
                txn.update_state()?;

                self.framed.send(Bytes::from(buf)).await.or_else(|e| {
                    Err(Error::PacketSendFailed(format!(
                        "Unable to send packet: {:?}",
                        e
                    )))
                })?;
            }
        }

        self.broker_tx
            .send(BrokerMsg::Publish {
                client: client_id,
                dup: publish.dup,
                qospid: publish.qospid,
                retain: publish.retain,
                topic_name: String::from(publish.topic_name),
                payload: publish.payload.to_vec(),
            })
            .await
            .or_else(|e| Err(Error::BrokerMsgSendFailure(format!("{:?}", e))))?;

        Ok(())
    }

    async fn handle_puback(&mut self, pid: &mqttrs::Pid) -> Result<()> {
        trace!("Received Puback packet from client {}.", self);

        // 1. find active txn with same pid
        let session = match self.get_session_mut() {
            Some(session) => session,
            None => {
                return Err(Error::ClientHandlerInvalidState(format!(
                    "ClientHandler {:?} received puback while not connected: {:?}",
                    self.addr, self.state
                )));
            }
        };

        let txn = match session.get_txn_mut(pid) {
            Some(txn) => txn,
            None => {
                // not finding a transaction should be a warning because it
                // should not cause either side to close the connection.
                warn!(
                    "Received puback for {:?} but could not find matching active transaction.",
                    pid
                );
                return Ok(());
            }
        };

        // 2. update state
        txn.update_state()?;
        if txn.current_state() != session::TransactionState::Puback {
            return Err(Error::MQTTProtocolViolation(format!(
                "Expected active transaction {:?} for {:?} to be in Puback state, but it was not.",
                txn, pid
            )));
        }

        trace!("Finishing active transaction {:?}", txn);

        // 3. close out txn
        session.finish_txn(pid)
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

        // TODO: validate subscribe packet format

        if subscribe.topics.len() == 0 {
            warn!("Received subscribe packet with no topics. Ignoring...");
            return Ok(());
        }

        let suback = Packet::Suback(mqttrs::Suback {
            pid: subscribe.pid,
            return_codes: subscribe
                .topics
                .iter()
                // TODO: return codes status and QoS level need to be calculated
                // based on client handler's internal Pid storage.
                .map(|_| mqttrs::SubscribeReturnCodes::Success(mqttrs::QoS::AtMostOnce))
                .collect(),
        });
        let buf_sz = if let Packet::Suback(ref suback) = suback {
            std::mem::size_of::<Packet>()
                + std::mem::size_of::<mqttrs::Suback>()
                + std::mem::size_of::<mqttrs::SubscribeReturnCodes>() * suback.return_codes.len()
        } else {
            0
        };
        let mut buf = vec![0u8; buf_sz];

        encode_slice(&suback, &mut buf as &mut [u8]).or_else(|e| {
            Err(Error::EncodeFailed(format!(
                "Unable to encode packet: {:?}",
                e
            )))
        })?;

        trace!(
            "Sending Suback packet to client {} in response: {:?}",
            self,
            suback
        );
        self.framed.send(Bytes::from(buf)).await.or_else(|e| {
            Err(Error::PacketSendFailed(format!(
                "Unable to send packet: {:?}",
                e
            )))
        })?;

        // now pass the subscribe packet back to shared broker state to handle
        // subscribe actions
        let session = match self.get_session_mut() {
            Some(session) => session,
            None => {
                return Err(Error::ClientHandlerInvalidState(format!(
                    "ClientHandler {:?} received subscribe while not connected: {:?}",
                    self.addr, self.state
                )));
            }
        };

        let client_id = session.id().to_string();

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
        Err(Error::MQTTProtocolViolation(format!(
            "Received Suback packet from client {}. This is not allowed. Closing connection: {:?}",
            self, suback
        )))
    }

    async fn handle_unsubscribe(&mut self, unsubscribe: &mqttrs::Unsubscribe) -> Result<()> {
        trace!("Received Unsubscribe packet from client {}.", self);

        if unsubscribe.topics.len() == 0 {
            return Err(Error::MQTTProtocolViolation(format!(
                "Received Unsubscribe packet with empty topic list. {:?}",
                unsubscribe
            )));
        }

        debug!(
            "Received unsubscribe request for the following topics: {}",
            unsubscribe.topics.join(", ")
        );

        // 1. send request to broker shared state to update subscriptions
        let session = match self.get_session() {
            Some(session) => session,
            None => {
                return Err(Error::ClientHandlerInvalidState(format!(
                    "ClientHandler {:?} received unsubscribe while not connected: {:?}",
                    self.addr, self.state
                )));
            }
        };
        let client_id = session.id().to_string();

        self.broker_tx
            .send(BrokerMsg::Unsubscribe {
                client: client_id,
                topics: unsubscribe.topics.clone(),
            })
            .await
            .or_else(|e| {
                Err(Error::BrokerMsgSendFailure(format!(
                    "Could not send BrokerMsg: {:?}",
                    e
                )))
            })?;

        // 2. send out unsuback to client
        let unsuback = Packet::Unsuback(unsubscribe.pid);
        let buf_sz = std::mem::size_of::<Packet>() + std::mem::size_of::<mqttrs::Pid>();
        let mut buf = vec![0u8; buf_sz];

        encode_slice(&unsuback, &mut buf as &mut [u8]).or_else(|e| {
            Err(Error::EncodeFailed(format!(
                "Unable to encode packet: {:?}",
                e
            )))
        })?;

        trace!(
            "Sending unsuback packet to client {} in response: {:?}",
            self,
            unsuback
        );
        self.framed.send(Bytes::from(buf)).await.or_else(|e| {
            Err(Error::PacketSendFailed(format!(
                "Unable to send packet: {:?}",
                e
            )))
        })?;

        Ok(())
    }

    async fn handle_unsuback(&mut self, pid: &mqttrs::Pid) -> Result<()> {
        Err(Error::MQTTProtocolViolation(format!(
            "Received Unsuback packet from client {}. This is not allowed. Closing connection: {:?}",
            self, pid
        )))
    }

    async fn handle_pingreq(&mut self) -> Result<()> {
        trace!("Received pingreq packet from client {}.", self);

        // respond to ping requests; will keep the connection alive
        let ping_resp = Packet::Pingresp {};
        let mut buf = vec![0u8; std::mem::size_of::<Packet>()];

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
        Err(Error::MQTTProtocolViolation(format!(
            "Received Pingresp packet from client {}. This is not allowed. Closing connection.",
            self
        )))
    }

    async fn handle_disconnect(&mut self) -> Result<()> {
        trace!("Received disconnect packet from client {}.", self);

        // if client is not connected, there is no session info, so we should
        // be fine doing nothing. This ClientHandler task will exit and everything
        // will be wrapped up.
        if let Some(session) = self.get_session() {
            self.broker_tx
                .send(BrokerMsg::ClientDisconnected {
                    client: session.id().to_string(),
                })
                .await
                .or_else(|e| Err(Error::BrokerMsgSendFailure(format!("{:?}", e))))?;
        }

        Ok(())
    }

    async fn decode_broker_msg(&mut self, msg: BrokerMsg) -> Result<()> {
        match msg {
            BrokerMsg::Publish { .. } => {
                self.handle_broker_publish(msg).await?;
                Ok(())
            }
            _ => {
                trace!(
                    "Ignoring unhandled message from shared broker state: {:?}",
                    msg
                );
                Ok(())
            }
        }
    }

    async fn handle_broker_publish(&mut self, publish: BrokerMsg) -> Result<()> {
        if let BrokerMsg::Publish {
            client: _,
            dup: _,
            qospid,
            retain,
            ref topic_name,
            ref payload,
        } = publish
        {
            let publish = Packet::Publish(mqttrs::Publish {
                dup: false,
                qospid,
                retain,
                topic_name,
                payload,
            });

            let buf_sz = if let Packet::Publish(ref publish) = publish {
                std::mem::size_of::<Packet>()
                    + std::mem::size_of::<mqttrs::Publish>()
                    + std::mem::size_of::<u8>() * publish.payload.len()
            } else {
                0
            };
            let mut buf = vec![0u8; buf_sz];

            encode_slice(&publish, &mut buf).or_else(|err| {
                Err(Error::EncodeFailed(format!(
                    "Unable to encode mqttrs packet: {:?}",
                    err
                )))
            })?;

            self.framed.send(Bytes::from(buf)).await.or_else(|err| {
                Err(Error::PacketSendFailed(format!(
                    "Unable to send packet: {:?}",
                    err
                )))
            })?;

            let session = match self.get_session_mut() {
                Some(session) => session,
                None => {
                    return Err(Error::ClientHandlerInvalidState(format!(
                        "ClientHandler {:?} trying to send publish while not connected: {:?}",
                        self.addr, self.state
                    )));
                }
            };

            match qospid {
                QosPid::AtMostOnce => (), // no follow up required
                QosPid::AtLeastOnce(pid) => {
                    // start record, need puback to update this
                    let _ = session.init_txn(pid, mqttrs::QoS::AtLeastOnce)?;
                    trace!("Starting QoS record for {:?}", qospid);

                    // TODO: implement timeout waiting for QoS responses?
                }
                QosPid::ExactlyOnce(pid) => {
                    // start record, need pubrel to update this
                    let _ = session.init_txn(pid, mqttrs::QoS::ExactlyOnce)?;
                    trace!("Starting QoS record for {:?}", qospid);

                    // TODO: implement timeout waiting for QoS responses?
                }
            }

            Ok(())
        } else {
            Err(Error::InvalidPacket(format!(
                "handle_broker_publish recieved invalid packet type: {:?}",
                publish
            )))
        }
    }
}
