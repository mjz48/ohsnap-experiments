use crate::error::{Error, Result};
use mqttrs::Pid;
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
    pub fn init_txn(&mut self, pid: Pid) -> Result<&Transaction> {
        let txn = Transaction::new(pid.clone());

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
        Ok(self.active_txns.get(&pid).unwrap())
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
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct Transaction {
    pid: Pid,
}

impl Transaction {
    pub fn new(pid: Pid) -> Transaction {
        Transaction { pid }
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
