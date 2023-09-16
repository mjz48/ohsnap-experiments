use crate::broker;
use crate::error::{Error, Result};
use log::{error, trace, warn};
use mqttrs::{Pid, QosPid};
use std::collections::HashMap;
use std::time::Duration;
use tokio::io::{Error as TokioError, ErrorKind};
use tokio::sync::mpsc::{self, error::SendError, Receiver, Sender};
use tokio::task::JoinHandle;
use tokio::time::sleep;

const TXN_CHANNEL_CAPACITY: usize = 5;

pub type QoSRespSender = Sender<Result<PacketData>>;
pub type QoSRespReceiver = Receiver<Result<PacketData>>;

/// Pass messages to the qos_tracker task
#[derive(Debug)]
pub enum QoSMsg {
    /// Initialize qos tracker with packet data. Only Publish/Pubrel allowed.
    Init(PacketData),
    /// Retry last send packet (QoS timeout has occurred)
    Retry(u32),
    /// Update qos tracker with packet data
    Update(PacketData),
}

/// Client session information struct
#[derive(Debug)]
pub struct Session {
    /// client identifier
    id: String,
    /// a copy of shared broker config
    config: broker::Config,
    /// list of active transactions needed for QoS handling
    active_txns: HashMap<Pid, (Sender<QoSMsg>, JoinHandle<()>)>,
    /// communication channel to client handler
    client_tx: QoSRespSender,
}

impl Session {
    /// Create a new Client Session. This contains identifying information and
    /// session data for MQTT protocol operations.
    ///
    /// # Arguments
    ///
    /// * `id` - String slice of client identifier
    pub fn new(id: &str, config: &broker::Config, tx: QoSRespSender) -> Session {
        Session {
            id: String::from(id),
            config: config.clone(),
            active_txns: HashMap::new(),
            client_tx: tx,
        }
    }

    /// Get an &str to the session's client id
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Add a new entry to active transactions to keep track of QoS handling
    ///
    /// # Arguments
    ///
    /// * `pid` - provide an available Pid to identify transaction
    /// * `qos` - the level of service of the transaction
    /// * `data` - data to recreate the necessary MQTT control packet
    ///
    /// # Errors
    ///
    /// This function may return the following exceptions:
    ///
    /// * MQTTProtocolViolation
    /// * ClientHandlerInvalidState
    pub async fn init_qos(&mut self, pid: Pid, data: PacketData) -> Result<()> {
        if self.is_pid_active(&pid) {
            return Err(Error::MQTTProtocolViolation(format!(
                "Trying to init new qos transaction with same pid ({:?}) as existing transaction.",
                pid
            )));
        }

        let (txn_tx, txn_rx) = mpsc::channel(TXN_CHANNEL_CAPACITY);

        let tracker = tokio::spawn(qos_tracker(
            self.id().to_string(),
            self.config.clone(),
            txn_rx,
            txn_tx.clone(),
            self.client_tx.clone(),
        ));
        self.active_txns.insert(pid, (txn_tx.clone(), tracker));

        send_to_tracker(&txn_tx, QoSMsg::Init(data)).await
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
    pub async fn update_qos(&mut self, pid: Pid, data: PacketData) -> Result<()> {
        if let Some((txn_tx, _)) = self.active_txns.get(&pid) {
            send_to_tracker(txn_tx, QoSMsg::Update(data)).await?;
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

impl Drop for Session {
    fn drop(&mut self) {
        // kill all spawned tasks in progress here
        for (_, (_, jh)) in &self.active_txns {
            jh.abort();
        }
    }
}

impl PartialEq for Session {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for Session {}

impl std::fmt::Display for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.id())
    }
}

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

/// Keep track of current step of QoS transaction
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QoSState {
    Publish(PacketData),
    Puback,
    Pubrec(PacketData),
    Pubrel(PacketData),
    Pubcomp,
}

/// For each active QoS transaction, spawn a separate async task to keep track
/// of current transaction state and handling timeouts and retries.
///
/// # Arguments
///
/// * `id` - client identifier for debug purposes
/// * `pid` - pid of transaction that this is tracking
/// * `qos` - qos level of transaction
/// * `rx` - channel receiver for receiving QoSMsg updates
/// * `tx` - channel for sending QoSMsg (for scheduling retries)
/// * `client_tx` - return channel to send results to session/client handler
async fn qos_tracker(
    id: String,
    config: broker::Config,
    mut rx: Receiver<QoSMsg>,
    tx: Sender<QoSMsg>,
    client_tx: QoSRespSender,
) {
    let mut state: Option<QoSState> = None;
    let mut retry_task: Option<JoinHandle<()>> = None;

    let retry_timeout = config.retry_interval as u64;
    let max_retries = config.max_retries as u32;

    while let Some(msg) = rx.recv().await {
        match msg {
            QoSMsg::Init(data) => {
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
                        state = Some(QoSState::Publish(PacketData::Publish {
                            dup: true,
                            qospid,
                            retain,
                            topic_name,
                            payload,
                        }));
                    }
                    PacketData::Pubrec(_) => {
                        state = Some(QoSState::Pubrec(data));
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

                retry_task = Some(send_retry(&tx, 0, retry_timeout));
            }
            QoSMsg::Retry(attempts) => {
                let attempt = attempts + 1;
                if attempt > max_retries {
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
                        Some(QoSState::Publish(ref data))
                        | Some(QoSState::Pubrel(ref data))
                        | Some(QoSState::Pubrec(ref data)) => Ok(data.clone()),
                        ref pkt => Err(Error::ClientHandlerInvalidState(format!(
                            "Trying to retry invalid packet: {:?}",
                            pkt
                        ))),
                    },
                )
                .await;

                // and then schedule next retry on timeout
                retry_task = Some(send_retry(&tx, attempt, retry_timeout));
                trace!(
                    "QoS timeout occurred for txn = {:?}, attempt = {}.",
                    state,
                    attempt
                );
            }
            QoSMsg::Update(data) => {
                // on update, cancel any pending retries, if it exists
                if let Some(ref task) = retry_task {
                    task.abort();
                }

                let expected_state = match state {
                    Some(QoSState::Publish(PacketData::Publish { qospid, .. })) => match qospid {
                        QosPid::ExactlyOnce(pid) => QoSState::Pubrec(PacketData::Pubrec(pid)),
                        QosPid::AtLeastOnce(_) => QoSState::Puback,
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
                    },
                    Some(QoSState::Pubrec(PacketData::Pubrec(_))) => QoSState::Pubcomp,
                    Some(QoSState::Pubrel(PacketData::Pubrel(_))) => QoSState::Pubcomp,
                    Some(QoSState::Pubcomp) => QoSState::Pubcomp,
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
                    QoSState::Publish(ref exp) => exp.clone(),
                    QoSState::Pubrel(ref exp) => exp.clone(),
                    QoSState::Pubrec(ref exp) => exp.clone(),
                    QoSState::Pubcomp => PacketData::Pubcomp(Pid::new()),
                    QoSState::Puback => PacketData::Puback(Pid::new()),
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
                    QoSState::Pubrec(PacketData::Pubrec(_)) => QoSState::Pubcomp,
                    QoSState::Puback | QoSState::Pubcomp => {
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
}

async fn send_to_tracker(tx: &Sender<QoSMsg>, msg: QoSMsg) -> Result<()> {
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

fn send_retry(tx: &Sender<QoSMsg>, retry: u32, delay: u64) -> JoinHandle<()> {
    let tx = tx.clone();
    tokio::spawn(async move {
        sleep(Duration::from_secs(delay)).await;

        tx.send(QoSMsg::Retry(retry))
            .await
            .or_else(|e| {
                eprintln!("send_retry could not send msg: {:?}", e);
                Ok::<(), SendError<QoSMsg>>(())
            })
            .unwrap();
    })
}
