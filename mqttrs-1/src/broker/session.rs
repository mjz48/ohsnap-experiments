use crate::{
    broker::{self, BrokerMsg},
    error::{Error, Result},
    mqtt,
};
use log::warn;
use mqtt::{LastWill, Packet, Pid};
use std::collections::HashMap;
use tokio::sync::mpsc::Sender;

pub mod qos;

/// Client session information struct
#[derive(Debug)]
pub struct Session {
    /// client identifier
    id: String,
    /// a copy of shared broker config
    config: broker::Config,
    /// store last will message of client, if any
    last_will: Option<LastWill>,
    /// channel sender to client handler (needed by QoS tracker to send retries)
    client_tx: Sender<BrokerMsg>,
    /// keep track of active QoS transactions
    qos_txns: HashMap<Pid, qos::Tracker>,
}

impl Session {
    /// Create a new Client Session. This contains identifying information and
    /// session data for MQTT protocol operations.
    ///
    /// # Arguments
    ///
    /// * `id` - String slice of client identifier
    pub fn new(
        id: &str,
        config: &broker::Config,
        last_will: Option<LastWill>,
        client_tx: Sender<BrokerMsg>,
    ) -> Session {
        Session {
            id: String::from(id),
            config: config.clone(),
            last_will,
            client_tx,
            qos_txns: HashMap::new(),
        }
    }

    /// Supply an update to a QoS transaction. If no transaction with the supplied
    /// Pid exists and the supplied mqtt::Packet is the correct type (Publish or Pubrec)
    /// this will create a new QoS tracker. Else, it will update an existing one.
    /// If a QoS update causes a transaction to complete, the session will
    /// automatically remove it from the active transactions list (qos_txns).
    ///
    /// # Arguments
    pub fn update_qos(&mut self, pid: Pid, packet: Packet) -> Result<()> {
        if !self.qos_txns.contains_key(&pid) {
            let tracker =
                match qos::Tracker::new(self.client_tx.clone(), &self.id, &self.config, packet) {
                    Ok(t) => t,
                    Err(err) => {
                        warn!("{:?}", err);
                        return Ok(());
                    }
                };

            // start a new transaction (if the packet type is incorrect, the
            // qos tracker will throw an error that must be propagated)
            self.qos_txns.insert(pid, tracker);
        } else {
            // update an existing one
            let tracker = if let Some(t) = self.qos_txns.get_mut(&pid) {
                t
            } else {
                return Err(Error::ClientHandlerInvalidState(format!(
                    "Error retrieving qos tracker for {:?}, state = {:?}",
                    pid, packet
                )));
            };

            match tracker.update(self.client_tx.clone(), &self.id, &self.config, packet) {
                Ok(qos::Update::Active) => (),
                Ok(qos::Update::Finished(pid)) => {
                    // the transaction is done, so remove it from the active list
                    self.qos_txns.remove(&pid);
                }
                Err(err) => {
                    warn!("{:?}", err);
                }
            }
        }
        Ok(())
    }

    /// Get an &str to the session's client id
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get last will message
    pub fn last_will(&self) -> Option<&LastWill> {
        self.last_will.as_ref().and_then(|lw| Some(lw))
    }
}

impl PartialEq for Session {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.config == other.config
    }
}

impl Eq for Session {}

impl std::fmt::Display for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.id())
    }
}
