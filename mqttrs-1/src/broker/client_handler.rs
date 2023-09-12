use crate::broker::session::{self, PacketData, TransactionState};
use crate::broker::{BrokerMsg, Config, Session};
use crate::error::{Error, Result};
use crate::mqtt;
use bytes::BytesMut;
use futures::StreamExt;
use log::{debug, info, trace, warn};
use mqttrs::{decode_slice, Packet, PacketType, QosPid};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{self, error::SendError, Sender};
use tokio::time::sleep;
use tokio_util::codec::{BytesCodec, Framed};

/// reserved capacity for broker shared state -> client handler channel
const BROKER_MSG_CAPACITY: usize = 100;

/// Actions to perform after handling received broker msgs.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum BrokerMsgAction {
    Exit,
    NoAction,
}

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
        if let Ok(session) = self.get_session() {
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

        // if the client opens a TCP connection but doesn't send a connect
        // packet within a "reasonable amount of time", the server SHOULD close
        // the connection. (That's what this next line does.)
        client.execute_after_delay(
            BrokerMsg::ClientConnectionTimeout {},
            Duration::from_secs(client.config.timeout_interval.into()),
        );

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
                            break client.on_client_disconnect().await;
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
                            match client.decode_broker_msg(msg).await {
                                Ok(BrokerMsgAction::Exit) => {
                                    break Ok(())
                                },
                                Ok(_) => (),
                                Err(err) => {
                                    break Err(err)
                                }
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
                client.on_client_disconnect().await?;
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
    async fn decode_packet<'pkt>(&mut self, buf: &'pkt BytesMut) -> Result<Option<Packet<'pkt>>> {
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
    async fn update_state(&mut self, pkt: &Packet<'_>) -> Result<()> {
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

                    // store client info and session data here (it's probably more logically clean
                    // to do this in handle_connect, but if we do it here we don't have to make
                    // ClientInfo an option.)
                    let info = Session::new(connect.client_id);
                    trace!("Creating new session data: {:?}", info);

                    // send init to broker shared state
                    self.send_broker(BrokerMsg::ClientConnected {
                        client: info.clone(),
                        client_tx: self.client_tx.clone(),
                    })
                    .await?;

                    // TODO: implement authentication + authorization support
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
    async fn on_client_disconnect(&self) -> Result<()> {
        if let ClientState::Connected(ref state) = self.state {
            self.send_broker(BrokerMsg::ClientDisconnected {
                client: state.id().to_string(),
            })
            .await
        } else {
            Ok(())
        }
    }

    /// Attempt to get the client session data.
    fn get_session(&self) -> Result<&Session> {
        if let ClientState::Connected(ref state) = self.state {
            Ok(state)
        } else {
            return Err(Error::ClientHandlerInvalidState(format!(
                "Could not get session data for client '{}'",
                self
            )));
        }
    }

    /// Attempt to get a mutable reference to the client session data
    fn get_session_mut(&mut self) -> Result<&mut Session> {
        if let ClientState::Connected(ref mut state) = self.state {
            Ok(state)
        } else {
            return Err(Error::ClientHandlerInvalidState(format!(
                "Could not get mutable session data for client '{}'",
                self
            )));
        }
    }

    /// Helper function to send a BrokerMsg to the shared broker state.
    async fn send_broker(&self, msg: BrokerMsg) -> Result<()> {
        self.broker_tx
            .send(msg)
            .await
            .map_err(|e| Error::BrokerMsgSendFailure(format!("Could not send broker msg: {:?}", e)))
    }

    /// Helper function to send an mqttrs packet to the client
    async fn send_client(&mut self, pkt: &Packet<'_>) -> Result<()> {
        mqtt::send(pkt, &mut self.framed).await
    }

    fn execute_after_delay(&self, msg: BrokerMsg, delay: Duration) {
        let client_tx = self.client_tx.clone();

        tokio::spawn(async move {
            sleep(delay).await;

            client_tx
                .send(msg)
                .await
                .or_else(|e| {
                    eprintln!("execute_after_delay could not send BrokerMsg: {:?}", e);
                    Ok::<(), SendError<BrokerMsg>>(())
                })
                .unwrap();
        });
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
    async fn handle_packet(&mut self, pkt: &Packet<'_>) -> Result<()> {
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

        trace!("Sending Connack packet in response: {:?}", connack);
        self.send_client(&connack).await
    }

    async fn handle_connack(&mut self, connack: &mqttrs::Connack) -> Result<()> {
        Err(Error::MQTTProtocolViolation(format!(
            "Received Connack packet from client {}. This is not allowed. Closing connection: {:?}",
            self, connack
        )))
    }

    async fn handle_publish(&mut self, publish: &mqttrs::Publish<'_>) -> Result<()> {
        trace!("Received Publish packet from client {}.", self);

        let session = self.get_session_mut()?;
        let client_id = session.id().to_string();

        match publish.qospid {
            mqttrs::QosPid::AtMostOnce => (), // no follow up required
            mqttrs::QosPid::AtLeastOnce(pid) => {
                // if QoS == 1, need to send PubAck, no need to initialize new
                // transaction since the required transmissions are already done
                let puback = Packet::Puback(pid);
                trace!(
                    "Sending puback response for QoS = {:?}: {:?}",
                    publish.qospid,
                    puback
                );
                self.send_client(&puback).await?;
            }
            mqttrs::QosPid::ExactlyOnce(pid) => {
                // initialize new transaction
                let _ = session.init_txn(pid, mqttrs::QoS::ExactlyOnce, PacketData::Pubrec(pid))?;

                let pubrec = Packet::Pubrec(pid);
                trace!(
                    "Sending pubrec response for QoS = {:?}: {:?}",
                    publish.qospid,
                    pubrec
                );
                self.send_client(&pubrec).await?;
            }
        }

        self.send_broker(BrokerMsg::Publish {
            client: client_id,
            dup: publish.dup,
            qospid: publish.qospid,
            retain: publish.retain,
            topic_name: String::from(publish.topic_name),
            payload: publish.payload.to_vec(),
        })
        .await
    }

    async fn handle_puback(&mut self, pid: &mqttrs::Pid) -> Result<()> {
        trace!("Received Puback packet from client {}.", self);

        // 1. find active txn with same pid
        let session = self.get_session_mut()?;
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
        txn.update_state(None)?;
        if txn.current_state() != &session::TransactionState::Puback {
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

        // TODO: implement SubscribeTopics and wildcard path validation

        if subscribe.topics.len() == 0 {
            return Err(Error::MQTTProtocolViolation(format!(
                "Received subscribe packet with no topics from client \
                '{}'. This is not allowed. Closing connection.",
                self
            )));
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
        trace!(
            "Sending Suback packet to client {} in response: {:?}",
            self,
            suback
        );
        self.send_client(&suback).await?;

        // now pass the subscribe packet back to shared broker state to handle
        // subscribe actions
        let session = self.get_session_mut()?;
        let client_id = session.id().to_string();

        self.send_broker(BrokerMsg::Subscribe {
            client: client_id,
            pid: subscribe.pid,
            topics: subscribe
                .topics
                .iter()
                .map(|ref subscribe_topic| subscribe_topic.topic_path.clone())
                .collect(),
        })
        .await
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
        let session = self.get_session()?;
        let client_id = session.id().to_string();

        self.send_broker(BrokerMsg::Unsubscribe {
            client: client_id,
            pid: unsubscribe.pid,
            topics: unsubscribe.topics.clone(),
        })
        .await?;

        // 2. send out unsuback to client
        let unsuback = Packet::Unsuback(unsubscribe.pid);
        trace!(
            "Sending unsuback packet to client {} in response: {:?}",
            self,
            unsuback
        );
        self.send_client(&unsuback).await
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
        self.send_client(&ping_resp).await
    }

    async fn handle_pingresp(&mut self) -> Result<()> {
        trace!("Received pingresp packet from client {}.", self);

        // the MQTT spec doesn't specify that servers can ping clients and that
        // clients have to respond. Technically, it should be okay to receive
        // these packets, but disallow in this implementation for security
        // purposes.
        Err(Error::MQTTProtocolViolation(format!(
            "Received Pingresp packet from client {}. Closing connection.",
            self
        )))
    }

    async fn handle_disconnect(&mut self) -> Result<()> {
        trace!("Received disconnect packet from client {}.", self);

        // if client is not connected, there is no session info, so we should
        // be fine doing nothing. This ClientHandler task will exit and everything
        // will be wrapped up.
        if let Ok(session) = self.get_session() {
            self.send_broker(BrokerMsg::ClientDisconnected {
                client: session.id().to_string(),
            })
            .await?;
        }

        Ok(())
    }

    async fn decode_broker_msg(&mut self, msg: BrokerMsg) -> Result<BrokerMsgAction> {
        match msg {
            BrokerMsg::Publish { .. } => self.handle_broker_publish(msg).await,
            BrokerMsg::ClientConnectionTimeout => self.handle_connection_timeout(),
            BrokerMsg::QoSTimeout { pid } => self.handle_qos_timeout(pid).await,
            _ => {
                trace!(
                    "Ignoring unhandled message from shared broker state: {:?}",
                    msg
                );
                Ok(BrokerMsgAction::NoAction)
            }
        }
    }

    async fn handle_broker_publish(&mut self, publish: BrokerMsg) -> Result<BrokerMsgAction> {
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
                qospid, // TODO: this actually needs to be a new Pid
                retain,
                topic_name,
                payload,
            });
            self.send_client(&publish).await?;

            match qospid {
                QosPid::AtMostOnce => (), // no follow up required
                QosPid::AtLeastOnce(pid) | QosPid::ExactlyOnce(pid) => {
                    let qos = match qospid {
                        QosPid::AtLeastOnce(_) => mqttrs::QoS::AtLeastOnce,
                        QosPid::ExactlyOnce(_) => mqttrs::QoS::ExactlyOnce,
                        _ => {
                            panic!("Received qospid in invalid state: {:?}", qospid);
                        }
                    };

                    let data = PacketData::Publish {
                        dup: false,
                        qospid,
                        retain,
                        topic_name: String::from(topic_name),
                        payload: payload.clone(),
                    };

                    // start record, need puback (QoS 1) or pubrec (QoS 2) to update this
                    let _ = self.get_session_mut()?.init_txn(pid, qos, data)?;
                    trace!("Starting QoS record for {:?}", qospid);

                    // handle retrying packet sends after timeout for QoS txns
                    self.execute_after_delay(
                        BrokerMsg::QoSTimeout { pid },
                        Duration::from_secs(self.config.retry_interval.into()),
                    );
                }
            }

            Ok(BrokerMsgAction::NoAction)
        } else {
            Err(Error::InvalidPacket(format!(
                "handle_broker_publish received invalid packet type: {:?}",
                publish
            )))
        }
    }

    fn handle_connection_timeout(&self) -> Result<BrokerMsgAction> {
        if let ClientState::Connected(ref session) = self.state {
            // client has connected in time, no action required
            trace!(
                "Client '{}' has successfully connected. Ignoring connection timeout callback.",
                session
            );
            Ok(BrokerMsgAction::NoAction)
        } else {
            trace!("ClientHandler timeout waiting for connection packet. Closing connection.");
            Ok(BrokerMsgAction::Exit)
        }
    }

    async fn handle_qos_timeout(&mut self, pid: mqttrs::Pid) -> Result<BrokerMsgAction> {
        let (should_retry, txn) = {
            let max_retries = self.config.max_retries;
            let session = match self.get_session_mut() {
                Ok(s) => s,
                // it is not an error if this happens, the connection may have
                // been closed for any reason before QoS callback fires.
                Err(_) => return Ok(BrokerMsgAction::NoAction),
            };
            (session.retry_txn(&pid, max_retries)?, session.get_txn(&pid))
        };

        if !should_retry || txn.is_none() {
            return Ok(BrokerMsgAction::NoAction);
        }
        let txn = txn.unwrap();

        match txn.current_state().clone() {
            TransactionState::Publish(PacketData::Publish {
                dup: _,
                qospid,
                retain,
                topic_name,
                payload,
            }) => {
                let publish = Packet::Publish(mqttrs::Publish {
                    dup: true,
                    qospid,
                    retain,
                    topic_name: topic_name.as_str(),
                    payload: &payload as &[u8],
                });
                trace!(
                    "Timeout waiting for QoS transaction response. Retransmitting {:?}",
                    publish
                );
                self.send_client(&publish).await?;
            }
            TransactionState::Pubrec(PacketData::Pubrec(pid)) => {
                let pubrec = Packet::Pubrec(pid);
                trace!(
                    "Timeout waiting for QoS transaction response. Retransmitting {:?}",
                    pubrec
                );
                self.send_client(&pubrec).await?;
            }
            TransactionState::Pubrel(PacketData::Pubrec(pid)) => {
                let pubrel = Packet::Pubrel(pid);
                trace!(
                    "Timeout waiting for QoS transaction response. Retransmitting {:?}",
                    pubrel
                );
                self.send_client(&pubrel).await?;
            }
            _ => {
                return Err(Error::ClientHandlerInvalidState(format!(
                    "QoS txn is in invalid state, txn = {:?}",
                    txn
                )));
            }
        };

        // fire another timeout callback
        self.execute_after_delay(
            BrokerMsg::QoSTimeout { pid },
            Duration::from_secs(self.config.retry_interval.into()),
        );

        Ok(BrokerMsgAction::NoAction)
    }
}
