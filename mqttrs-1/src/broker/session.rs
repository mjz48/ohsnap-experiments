use crate::error::{Error, Result};
use log::trace;
use mqttrs::{Pid, QoS, QosPid};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// Client session information struct
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Session {
    /// client identifier
    id: String,
    /// list of active transactions needed for QoS handling
    active_txns: HashMap<Pid, Transaction>,
}

impl std::fmt::Display for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.id())
    }
}

impl Session {
    /// Create a new Client Session. This contains identifying information and
    /// session data for MQTT protocol operations.
    ///
    /// # Arguments
    ///
    /// * `id` - String slice of client identifier
    pub fn new(id: &str) -> Session {
        Session {
            id: String::from(id),
            active_txns: HashMap::new(),
        }
    }

    /// Get an &str to the session's client id
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Add a new entry to active transactions to keep track of QoS handling
    pub fn init_txn(&mut self, pid: Pid, qos: QoS, data: PacketData) -> Result<&mut Transaction> {
        let txn = Transaction::new(pid.clone(), qos, data)?;

        if self.is_pid_already_active(&pid) {
            return Err(
                Error::MQTTProtocolViolation(
                    format!(
                        "Trying to init new transaction with same pid ({:?}) as existing transaction ({:?}).",
                        pid,
                        self.get_txn(&pid).expect("Pid was guaranteed to be in this HashMap")
                    )
                ));
        }

        self.active_txns.insert(pid.clone(), txn);
        Ok(self.active_txns.get_mut(&pid).unwrap())
    }

    /// Delete transaction session data without checking to see if it finished
    /// successfully. Call this in cases where there is transmission failure
    /// or clean up needed after error handling.
    pub fn abort_txn(&mut self, pid: &Pid) -> Result<()> {
        let txn = match self.active_txns.remove(pid) {
            Some(txn) => txn,
            None => return Ok(()),
        };

        trace!("Aborting QoS Transaction: {:?}", txn);
        Ok(())
    }

    /// Mark this transaction as completed and remove it from the session. This
    /// represents the point where the Pid of the transaction can be reused.
    pub fn finish_txn(&mut self, pid: &Pid) -> Result<()> {
        let txn = match self.active_txns.remove(pid) {
            Some(txn) => txn,
            None => return Ok(()),
        };

        let state = txn.current_state();
        if state != &TransactionState::Puback && state != &TransactionState::Pubcomp {
            return Err(Error::MQTTProtocolViolation(format!(
                "Trying to close transaction that is not in finished state! {:?}",
                txn
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
    pub fn is_pid_already_active(&self, pid: &Pid) -> bool {
        self.active_txns.contains_key(pid)
    }

    /// Obtain a reference to an active transaction by pid.
    ///
    /// # Arguments
    ///
    /// * `pid` - mqttrs::Pid to search for
    ///
    pub fn get_txn(&self, pid: &Pid) -> Option<&Transaction> {
        self.active_txns.get(pid)
    }

    /// Obtain a mutable reference to an active transaction by pid.
    ///
    /// # Arguments
    ///
    /// * `pid` - mqttrs::Pid to search for
    ///
    pub fn get_txn_mut(&mut self, pid: &Pid) -> Option<&mut Transaction> {
        self.active_txns.get_mut(pid)
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
    Pubrec(Pid),
    Pubrel(Pid),
}

/// Keep track of current step of QoS transaction
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionState {
    Publish(PacketData),
    Puback,
    Pubrec(PacketData),
    Pubrel(PacketData),
    Pubcomp,
}

/// Bookkeeping struct to keep track of in progress QoS transactions
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pid: Pid,
    qos: QoS,
    num_retries: u16,
    state: TransactionState,
}

impl Transaction {
    pub fn new(pid: Pid, qos: QoS, data: PacketData) -> Result<Transaction> {
        let state = match data {
            PacketData::Publish { .. } => TransactionState::Publish(data),
            PacketData::Pubrec(_) => TransactionState::Pubrec(data),
            _ => {
                return Err(Error::ClientHandlerInvalidState(format!(
                    "Can't create QoS record with packet data: {:?}",
                    data
                )));
            }
        };

        Ok(Transaction {
            pid,
            qos,
            num_retries: 0,
            state,
        })
    }

    pub fn get_retries(&self) -> u16 {
        self.num_retries
    }

    pub fn get_retries_mut(&mut self) -> &mut u16 {
        &mut self.num_retries
    }

    pub fn current_state(&self) -> &TransactionState {
        &self.state
    }

    pub fn update_state(&mut self, data: Option<PacketData>) -> Result<TransactionState> {
        let prev_state = self.state.clone();
        match self.state {
            TransactionState::Publish(_) => match self.qos {
                mqttrs::QoS::AtLeastOnce => {
                    self.state = TransactionState::Puback;
                }
                mqttrs::QoS::ExactlyOnce => {
                    if data.is_none() {
                        return Err(Error::ClientHandlerInvalidState(format!(
                            "Can't update txn to pubrel without data, txn = {:?}",
                            self
                        )));
                    }
                    let data = data.unwrap();

                    if let PacketData::Pubrel(_) = data {
                        self.state = TransactionState::Pubrel(data);
                    } else {
                        return Err(Error::ClientHandlerInvalidState(format!(
                            "Can't update txn to pubrel with following data: {:?}, txn = {:?}",
                            data, self
                        )));
                    }
                }
                mqttrs::QoS::AtMostOnce => {
                    return Err(Error::MQTTProtocolViolation(format!(
                        "QoS Transaction has invalid QoS value ({:?}): {:?}",
                        self.qos, self
                    )));
                }
            },
            TransactionState::Puback => {
                return Err(Error::MQTTProtocolViolation(format!(
                    "Transaction in Puback state was called to update. {:?}",
                    self
                )));
            }
            TransactionState::Pubrec(_) => {
                if data.is_some() {
                    return Err(Error::ClientHandlerInvalidState(format!(
                        "Update to pubcomp does not take data: {:?}, txn = {:?}",
                        data, self
                    )));
                }
                self.state = TransactionState::Pubcomp;
            }
            TransactionState::Pubrel(_) => {
                if data.is_some() {
                    return Err(Error::ClientHandlerInvalidState(format!(
                        "Update txn to pubcomp does not take data: {:?}, txn = {:?}",
                        data, self
                    )));
                }
                self.state = TransactionState::Pubcomp;
            }
            TransactionState::Pubcomp => {
                return Err(Error::MQTTProtocolViolation(format!(
                    "Transaction in Pubcomp state was called to update. {:?}",
                    self
                )));
            }
        }

        return Ok(prev_state);
    }
}

impl Hash for Transaction {
    fn hash<H>(&self, h: &mut H)
    where
        H: Hasher,
    {
        self.pid.hash(h)
    }
}
