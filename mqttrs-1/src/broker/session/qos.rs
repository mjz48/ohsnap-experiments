use crate::broker;
use crate::error::{Error, Result};
use log::{error, trace, warn};
use mqttrs::{Pid, QosPid};
use std::collections::HashMap;
use std::time::Duration;
use tokio::io::{Error as TokioError, ErrorKind};
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::task::JoinHandle;
use tokio::time::sleep;

/// tokio mpsc channel capacity for Tracker transaction instances
const QOS_CHANNEL_CAPACITY: usize = 5;

pub type QoSRespSender = Sender<Result<PacketData>>;
pub type QoSRespReceiver = Receiver<Result<PacketData>>;

/// container for MQTT control packets. Need to store packet information so we
/// can retry. It's really hard to use the mqttrs::Publish packet because it
/// has lots of references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PacketData {
    Publish {
        dup: bool,
        qospid: QosPid,
        retain: bool,
        topic_name: String,
        payload: Vec<u8>,
    },
    Puback(Pid),
    Pubrec(Pid),
    Pubrel(Pid),
    Pubcomp(Pid),
}

/// QoS messages. These are used between the client handler and the QoS tracker
/// (via the Tracker interface) to both update QoS tracker information and
/// cause the client handler to send retry packets to the client.
#[derive(Debug)]
pub enum Msg {
    /// Initialize qos tracker with packet data. Only Publish/Pubrel allowed.
    Init(PacketData),
    /// Retry last send packet (QoS timeout has occurred)
    Retry(u32),
    /// Update qos tracker with packet data
    Update(PacketData),
}

/// Keep track of current step of QoS transaction
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Publish(PacketData),
    Puback,
    Pubrec(PacketData),
    Pubrel(PacketData),
    Pubcomp,
}

/// Struct to encapsulate state machines to keep track of QoS transactions
#[derive(Debug)]
pub struct Tracker {
    /// copy of the client identifier
    id: String,
    /// a copy of the shared broker config
    config: broker::Config,
    /// communication back to client handler
    client_tx: QoSRespSender,
    /// list of active transactions needed for QoS handling
    active_txns: HashMap<Pid, (Sender<Msg>, JoinHandle<()>)>,
}

impl Tracker {
    /// Create a new Tracker that will keep track of all QoS transactions
    /// currently in flight.
    ///
    /// # Arguments
    ///
    /// * `id` - reference to client identifier (for debug purposes)
    /// * `config` - reference to shared broker config
    /// * `client_tx` - channel sender to client handler
    pub fn new(id: &str, config: &broker::Config, client_tx: QoSRespSender) -> Tracker {
        Tracker {
            id: id.to_string(),
            config: config.clone(),
            client_tx,
            active_txns: HashMap::new(),
        }
    }

    /// For each active QoS transaction, spawn a separate async task to keep track
    /// of current transaction state and handling timeouts and retries.
    ///
    /// # Arguments
    ///
    /// * `pid` - pid of transaction that this is tracking
    /// * `data` - packet data of initial transaction state
    pub async fn start(&mut self, pid: Pid, data: PacketData) -> Result<()> {
        if self.is_pid_active(&pid) {
            return Err(Error::MQTTProtocolViolation(format!(
                "Trying to init new qos transaction with same pid ({:?}) as existing transaction.",
                pid
            )));
        }
        // create channel to send/receive QoS tracker messages (qos::Msg)
        let (qos_tx, mut qos_rx) = mpsc::channel(QOS_CHANNEL_CAPACITY);

        let id = self.id.to_string();
        let retry_timeout = self.config.retry_interval as u64;
        let max_retries = self.config.max_retries as u32;
        let client_tx = self.client_tx.clone();

        let tracker_qos_tx = qos_tx.clone();

        let tracker = tokio::spawn(async move {
            let mut state: Option<State> = None;
            let mut retry_task: Option<JoinHandle<()>> = None;

            while let Some(msg) = qos_rx.recv().await {
                match msg {
                    Msg::Init(data) => {
                        if state.is_some() {
                            send_to_client(
                                    &client_tx,
                                    Err(Error::ClientHandlerInvalidState(format!(
                                        "Trying to initialize QoS transaction that is already initialized: client = '{}', state = {:?}",
                                        id, state
                                    ))),
                                ).await;
                            return;
                        }
                        trace!(
                            "Starting new QoS Tracker with client = '{}', data = {:?}",
                            id,
                            data
                        );

                        match data {
                            PacketData::Publish {
                                dup: _,
                                qospid,
                                retain,
                                topic_name,
                                payload,
                            } => {
                                state = Some(State::Publish(PacketData::Publish {
                                    dup: true,
                                    qospid,
                                    retain,
                                    topic_name,
                                    payload,
                                }));
                            }
                            PacketData::Pubrec(_) => {
                                state = Some(State::Pubrec(data));
                            }
                            _ => {
                                send_to_client(
                                        &client_tx,
                                        Err(Error::MQTTProtocolViolation(format!(
                                            "Can't start QoS transaction with packet: client = '{}', data = {:?}",
                                            id, data
                                        ))),
                                    )
                                    .await;
                                return;
                            }
                        }

                        retry_task = Some(send_retry(&qos_tx, 0, retry_timeout));
                    }
                    Msg::Retry(attempts) => {
                        let attempt = attempts + 1;
                        if max_retries != 0 && attempt > max_retries {
                            // max attempts reached, abort
                            warn!(
                                "QoS transaction failed after max retransmission attempts. \
                                     Aborting transaction: {:?}",
                                state
                            );
                            return;
                        }

                        // send packet to client handler to retransmission
                        send_to_client(
                            &client_tx,
                            match state {
                                Some(State::Publish(ref data))
                                | Some(State::Pubrel(ref data))
                                | Some(State::Pubrec(ref data)) => Ok(data.clone()),
                                ref pkt => Err(Error::ClientHandlerInvalidState(format!(
                                    "Trying to retry invalid packet: {:?}",
                                    pkt
                                ))),
                            },
                        )
                        .await;

                        // and then schedule next retry on timeout
                        retry_task = Some(send_retry(&qos_tx, attempt, retry_timeout));
                        trace!(
                            "QoS timeout occurred for txn = {:?}, attempt = {}.",
                            state,
                            attempt
                        );
                    }
                    Msg::Update(data) => {
                        // on update, cancel any pending retries, if it exists
                        if let Some(ref task) = retry_task {
                            task.abort();
                        }

                        let expected_state = match state {
                            Some(State::Publish(PacketData::Publish { qospid, .. })) => {
                                match qospid {
                                    QosPid::ExactlyOnce(pid) => {
                                        State::Pubrec(PacketData::Pubrec(pid))
                                    }
                                    QosPid::AtLeastOnce(_) => State::Puback,
                                    QosPid::AtMostOnce => {
                                        return send_to_client(
                                            &client_tx,
                                            Err(Error::ClientHandlerInvalidState(format!(
                                            "Trying to update QoS transaction with invalid packet state: {:?}",
                                            data
                                        ))),
                                        )
                                        .await;
                                    }
                                }
                            }
                            Some(State::Pubrec(PacketData::Pubrec(_))) => State::Pubcomp,
                            Some(State::Pubrel(PacketData::Pubrel(_))) => State::Pubcomp,
                            Some(State::Pubcomp) => State::Pubcomp,
                            Some(_) | None => {
                                return send_to_client(
                                    &client_tx,
                                    Err(Error::ClientHandlerInvalidState(format!(
                                        "Trying to update QoS transaction with invalid packet state: {:?}",
                                        data
                                    ))),
                                )
                                .await;
                            }
                        };

                        let expected_pid = match data {
                            PacketData::Publish { ref qospid, .. } => match qospid {
                                QosPid::AtLeastOnce(ref pid) => pid,
                                QosPid::ExactlyOnce(ref pid) => pid,
                                _ => {
                                    panic!("Unexpected packet data while getting pid: {:?}", data);
                                }
                            },
                            PacketData::Pubrec(ref pid) => pid,
                            PacketData::Pubrel(ref pid) => pid,
                            PacketData::Pubcomp(ref pid) => pid,
                            PacketData::Puback(ref pid) => pid,
                        };
                        let expected_data = match expected_state {
                            State::Publish(ref exp) => exp.clone(),
                            State::Pubrel(ref exp) => exp.clone(),
                            State::Pubrec(ref exp) => exp.clone(),
                            State::Pubcomp => PacketData::Pubcomp(Pid::new()),
                            State::Puback => PacketData::Puback(Pid::new()),
                        };

                        if expected_data != data {
                            return send_to_client(
                                &client_tx,
                                Err(Error::ClientHandlerInvalidState(format!(
                                    "QoS transaction update does not match expected: \
                                         expected = {:?}, actual = {:?}",
                                    expected_data, data
                                ))),
                            )
                            .await;
                        }

                        let updated_state = match expected_state {
                            State::Pubrec(PacketData::Pubrec(_)) => State::Pubcomp,
                            State::Puback | State::Pubcomp => {
                                trace!("QoS update for QoS transaction '{}',{:?} completed. Finalizing transaction.", id, expected_pid);
                                return;
                            }
                            state => state,
                        };

                        state = Some(updated_state);
                        trace!(
                            "QoS update for QoS transaction '{}',{:?} completed. Next expected state is: {:?}",
                            id,
                            expected_pid,
                            state
                        );
                    }
                }
            }
        });

        self.active_txns
            .insert(pid, (tracker_qos_tx.clone(), tracker));
        send_to_tracker(&tracker_qos_tx, Msg::Init(data)).await
    }

    /// Update an existing qos tracker entry. This will also cause the tracker
    /// to automatically exit if all expected updates have been provided.
    ///
    /// # Arguments
    ///
    /// * `pid` - pid of transaction to update
    /// * `data` - packet data of latest step of transaction
    ///
    /// # Errors
    ///
    /// This function may throw the following errors:
    ///
    /// * ClientHandlerInvalidState
    /// * TokioErr
    pub async fn update(&mut self, pid: Pid, data: PacketData) -> Result<()> {
        if let Some((txn_tx, _)) = self.active_txns.get(&pid) {
            send_to_tracker(txn_tx, Msg::Update(data)).await?;
        } else {
            return Err(Error::ClientHandlerInvalidState(format!(
                "update_qos: no transaction with pid {:?} exists.",
                pid
            )));
        }

        Ok(())
    }

    /// Check if the given Pid is already in active transactions list. The MQTT
    /// spec requires that all separate transactions have unique Pids. Pids
    /// are able to be re-used when the transaction finishes.
    ///
    /// # Arguments
    ///
    /// * `pid` - mqttrs::Pid to search for
    ///
    pub fn is_pid_active(&self, pid: &Pid) -> bool {
        if let Some((_, jh)) = self.active_txns.get(pid) {
            return !jh.is_finished();
        }
        return false;
    }
}

impl Drop for Tracker {
    fn drop(&mut self) {
        // kill all spawned tasks in progress here
        for (_, (_, jh)) in &self.active_txns {
            jh.abort();
        }
    }
}

impl PartialEq for Tracker {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for Tracker {}

impl std::fmt::Display for Tracker {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Tracker{{client({})}}", self.id)
    }
}

async fn send_to_tracker(tx: &Sender<Msg>, msg: Msg) -> Result<()> {
    if let Err(err) = tx.send(msg).await {
        Err(Error::TokioErr(TokioError::new(ErrorKind::Other, err)))
    } else {
        Ok(())
    }
}

async fn send_to_client(tx: &QoSRespSender, msg: Result<PacketData>) {
    // TODO: make qos_tracker a struct and abort the retry task here
    if let Err(err) = tx.send(msg).await {
        error!(
            "send_to_client: Could not send message to client: {:?}",
            err
        );
    }
}

fn send_retry(tx: &Sender<Msg>, retry: u32, delay: u64) -> JoinHandle<()> {
    let tx = tx.clone();
    tokio::spawn(async move {
        sleep(Duration::from_secs(delay)).await;

        tx.send(Msg::Retry(retry))
            .await
            .or_else(|e| {
                eprintln!("send_retry could not send msg: {:?}", e);
                Ok::<(), SendError<Msg>>(())
            })
            .unwrap();
    })
}
