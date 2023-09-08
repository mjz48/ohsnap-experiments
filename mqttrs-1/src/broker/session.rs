use crate::error::{Error, Result};
use mqttrs::{Pid, QoS};
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
    pub fn init_txn(&mut self, pid: Pid, qos: QoS) -> Result<&mut Transaction> {
        let txn = Transaction::new(pid.clone(), qos);

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

    /// Mark this transaction as completed and remove it from the session. This
    /// represents the point where the Pid of the transaction can be reused.
    pub fn finish_txn(&mut self, pid: &Pid) -> Result<()> {
        let txn = match self.active_txns.remove(pid) {
            Some(txn) => txn,
            None => return Ok(()),
        };

        let state = txn.current_state();
        if state != TransactionState::Puback && state != TransactionState::Pubcomp {
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

/// Bookkeeping struct to keep track of in progress QoS transactions
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Transaction {
    pid: Pid,
    qos: QoS,
    state: TransactionState,
}

impl Transaction {
    pub fn new(pid: Pid, qos: QoS) -> Transaction {
        Transaction {
            pid,
            qos,
            state: TransactionState::Publish,
        }
    }

    pub fn current_state(&self) -> TransactionState {
        self.state
    }

    pub fn update_state(&mut self) -> Result<TransactionState> {
        let prev_state = self.state;
        match self.state {
            TransactionState::Publish => match self.qos {
                mqttrs::QoS::AtLeastOnce => {
                    self.state = TransactionState::Puback;
                }
                mqttrs::QoS::ExactlyOnce => {
                    self.state = TransactionState::Pubrel;
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
            TransactionState::Pubrec => {
                self.state = TransactionState::Pubrel;
            }
            TransactionState::Pubrel => {
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

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TransactionState {
    Publish,
    Puback,
    Pubrec,
    Pubrel,
    Pubcomp,
}
