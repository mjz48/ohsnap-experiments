// TODO: get rid of this during QoS 2 rewrite
pub use crate::broker::session::qos::{QoSRespReceiver, QoSRespSender};

use crate::{broker, error::Result, mqtt};
use mqtt::{Packet, Pid};

pub mod qos;

/// Client session information struct
#[derive(Debug, PartialEq, Eq)]
pub struct Session {
    /// client identifier
    id: String,
    /// a copy of shared broker config
    config: broker::Config,
    /// submodule to keep track of QoS transaction state
    qos: qos::Tracker,
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
            qos: qos::Tracker::new(id, config, tx),
        }
    }

    /// Start tracking QoS protocol for a specific transaction.
    ///
    /// # Arguments
    ///
    /// * `pid` - pid of transaction that this is tracking
    /// * `data` - packet data of initial transaction state
    pub async fn start_qos(&mut self, pid: Pid, data: Packet) -> Result<()> {
        self.qos.start(pid, data).await
    }

    /// Update a QoS protocol tracker for a specific transaction.
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
    pub async fn update_qos(&mut self, pid: Pid, data: Packet) -> Result<()> {
        self.qos.update(pid, data).await
    }

    /// Get an &str to the session's client id
    pub fn id(&self) -> &str {
        &self.id
    }
}

impl std::fmt::Display for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.id())
    }
}
