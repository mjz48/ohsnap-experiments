use crate::{
    broker,
    error::{Error, Result},
    mqtt::{self, Packet, Pid, QosPid},
};
use log::{error, trace, warn};
use std::{collections::HashMap, time::Duration};
use tokio::{
    io::{Error as TokioError, ErrorKind},
    sync::mpsc::{self, error::SendError, Receiver, Sender},
    task::JoinHandle,
    time::sleep,
};

/// tokio mpsc channel capacity for Tracker transaction instances
const QOS_CHANNEL_CAPACITY: usize = 5;

pub type QoSRespSender = Sender<Result<Packet>>;
pub type QoSRespReceiver = Receiver<Result<Packet>>;

/// QoS messages. These are used between the client handler and the QoS tracker
/// (via the Tracker interface) to both update QoS tracker information and
/// cause the client handler to send retry packets to the client.
#[derive(Debug)]
pub enum Msg {
    /// Initialize qos tracker with packet data. Only Publish/Pubrel allowed.
    Init(Packet),
    /// Retry last send packet (QoS timeout has occurred)
    Retry(u32),
    /// Update qos tracker with packet data
    Update(Packet),
}

/// Keep track of current step of QoS transaction
#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Publish(Packet),
    Puback,
    Pubrec(Packet),
    Pubrel(Packet),
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
    pub async fn start(&mut self, pid: Pid, data: Packet) -> Result<()> {
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
                            return send_to_client(
                                &client_tx,
                                Err(Error::ClientHandlerInvalidState(format!(
                                    "Trying to initialize QoS transaction that is \
                                     already initialized: client = '{}', state = {:?}",
                                    id, state
                                ))),
                            )
                            .await;
                        }
                        trace!(
                            "Starting new QoS Tracker with client = '{}', data = {:?}",
                            id,
                            data
                        );

                        match data {
                            Packet::Publish(_) => state = Some(State::Publish(data)),
                            Packet::Pubrec(_) => state = Some(State::Pubrec(data)),
                            _ => {
                                return send_to_client(
                                    &client_tx,
                                    Err(Error::MQTTProtocolViolation(format!(
                                        "Can't start QoS transaction with packet: client = '{}', data = {:?}",
                                        id, data
                                    ))),
                                )
                                .await;
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

                        let (expected_state, pid) = match state {
                            Some(State::Publish(Packet::Publish(publish))) => {
                                match publish.qospid {
                                    QosPid::ExactlyOnce(pid) => {
                                        (State::Pubrec(Packet::Pubrec(pid)), pid)
                                    }
                                    QosPid::AtLeastOnce(pid) => (State::Puback, pid),
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
                            Some(State::Pubrec(Packet::Pubrec(pid))) => (State::Pubcomp, pid),
                            Some(State::Pubrel(Packet::Pubrel(pid))) => (State::Pubcomp, pid),
                            Some(State::Pubcomp) => (State::Pubcomp, Pid::new()),
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

                        let expected_pkt = match expected_state {
                            State::Publish(ref pkt) => pkt.clone(),
                            State::Puback => Packet::Puback(pid),
                            State::Pubrec(ref pkt) => pkt.clone(),
                            State::Pubrel(ref pkt) => pkt.clone(),
                            State::Pubcomp => Packet::Pubcomp(pid),
                        };

                        if expected_pkt != data {
                            return send_to_client(
                                &client_tx,
                                Err(Error::ClientHandlerInvalidState(format!(
                                    "QoS transaction update does not match expected: \
                                         expected = {:?}, actual = {:?}",
                                    expected_pkt, data
                                ))),
                            )
                            .await;
                        }

                        let updated_state = match expected_state {
                            State::Pubrec(Packet::Pubrec(_)) => State::Pubcomp,
                            State::Puback | State::Pubcomp => {
                                trace!("QoS update for QoS transaction '{}',{:?} completed. Finalizing transaction.", id, pid);
                                return;
                            }
                            state => state,
                        };

                        state = Some(updated_state);
                        trace!(
                            "QoS update for QoS transaction '{}',{:?} completed. Next expected state is: {:?}",
                            id,
                            pid,
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
    pub async fn update(&mut self, pid: Pid, data: Packet) -> Result<()> {
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

async fn send_to_client(tx: &QoSRespSender, msg: Result<Packet>) {
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
