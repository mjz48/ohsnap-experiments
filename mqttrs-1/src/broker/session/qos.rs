use crate::{
    broker::{self, BrokerMsg},
    error::{Error, Result},
    mqtt::{Packet, Pid, QosPid},
};
use log::{debug, trace, warn};
use tokio::{
    sync::mpsc::{error::SendError, Sender},
    task::JoinHandle,
    time::{sleep, Duration},
};

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

/// Status result for QoS updates. If this returns Finished, it means the previous
/// update caused the QoS transaction to complete and it can be safely deleted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Update {
    Active,
    Finished(Pid),
}

/// Struct to encapsulate state machines to keep track of QoS transactions
#[derive(Debug)]
pub struct Tracker {
    /// last seen state of the QoS transaction
    state: Packet,
    /// join handle of active retry process
    retry_task: Option<JoinHandle<()>>,
}

impl Tracker {
    /// Create a new Tracker that will keep track of a QoS transaction protocol
    /// currently in flight.
    ///
    /// # Arguments
    ///
    /// * `client_tx` - channel sender to client handler (for retries)
    /// * `id` - client identifier
    /// * `config` - reference to shared broker config
    /// * `packet` - mqtt::Packet with starting state information
    pub fn new(
        client_tx: Sender<BrokerMsg>,
        id: &str,
        config: &broker::Config,
        packet: Packet,
    ) -> Result<Tracker> {
        let state = match packet {
            Packet::Publish(_) | Packet::Pubrec(_) => packet,
            _ => {
                return Err(Error::MQTTProtocolViolation(format!(
                    "Can't start QoS transaction with packet: {:?}",
                    packet
                )))
            }
        };

        let retry_task = spawn_retry_task(client_tx.clone(), id, config, state.clone());

        debug!(
            "Starting new QoS tracker for client '{}' with state = {:?}",
            id, state
        );

        Ok(Tracker {
            state,
            retry_task: Some(retry_task),
        })
    }

    /// Update this tracker with new state.
    ///
    /// # Arguments
    ///
    /// * `client_tx` - channel sender to client handler (for retries)
    /// * `id` - client identifier
    /// * `config` - reference to shared broker config
    /// * `packet` - mqtt::Packet with starting state information
    pub fn update(
        &mut self,
        client_tx: Sender<BrokerMsg>,
        id: &str,
        config: &broker::Config,
        packet: Packet,
    ) -> Result<Update> {
        // abort existing retry task, if it exists
        if let Some(ref retry_task) = self.retry_task {
            retry_task.abort();
        }
        self.retry_task = None;

        // validate the update packet
        let expected_packet = match self.state {
            Packet::Publish(ref publish) => match publish.qospid {
                QosPid::AtMostOnce => {
                    return Err(Error::ClientHandlerInvalidState(format!(
                        "QoS transaction has invalid qos value: {:?}",
                        self.state
                    )))
                }
                QosPid::AtLeastOnce(pid) => Packet::Puback(pid),
                QosPid::ExactlyOnce(pid) => Packet::Pubrec(pid),
            },
            Packet::Pubrec(ref pid) => Packet::Pubrel(pid.clone()),
            Packet::Pubrel(ref pid) => Packet::Pubcomp(pid.clone()),
            _ => {
                return Err(Error::ClientHandlerInvalidState(format!(
                    "Trying to update QoS transaction with invalid state: {:?}",
                    packet
                )));
            }
        };

        if expected_packet != packet {
            return Err(Error::MQTTProtocolViolation(format!(
                "Received QoS update with invalid state. client = '{}', Expected = {:?}, actual = {:?}",
                id, expected_packet, packet
            )));
        }

        self.state = expected_packet;
        trace!(
            "Updating QoS transaction for client '{}'. New state = {:?}",
            id,
            self.state
        );

        match self.state {
            Packet::Puback(pid) | Packet::Pubcomp(pid) => {
                trace!(
                    "QoS transaction for client '{}' complete: {:?}",
                    id,
                    self.state
                );
                return Ok(Update::Finished(pid));
            }
            _ => (),
        }

        // don't forget to schedule retries
        self.retry_task = Some(spawn_retry_task(
            client_tx.clone(),
            id,
            config,
            packet.clone(),
        ));

        Ok(Update::Active)
    }
}

impl Drop for Tracker {
    fn drop(&mut self) {
        if let Some(ref retry_task) = self.retry_task {
            retry_task.abort();
        }
    }
}

/// Create a new async task to wait on an interval and then resend packets
/// to the client for QoS related timeout retransmissions.
///
/// # Arguments
///
/// * `tx` - BrokerMsg channel sender to client handler for QoS retries
/// * `id` - client identifier
/// * `config` - broker config (for retry parameters)
/// * `packet` - packet data to resend
fn spawn_retry_task(
    tx: Sender<BrokerMsg>,
    id: &str,
    config: &broker::Config,
    packet: Packet,
) -> JoinHandle<()> {
    let id = id.to_string();
    let max_retries = config.max_retries;
    let interval = Duration::from_secs(config.retry_interval.into());

    let mut attempts = 0;
    let msg = BrokerMsg::QoSRetry {
        client: id.clone(),
        packet,
    };

    tokio::spawn(async move {
        loop {
            if max_retries != 0 && attempts > max_retries {
                // max attempts reached, abort
                warn!(
                    "QoS transaction failed after max retransmission attempts. \
                            Aborting transaction: {:?}",
                    msg,
                );
                return;
            }
            sleep(interval).await;

            trace!(
                "QoS timeout occurred for client '{}'. Retransmitting packet: {:?}",
                id,
                msg
            );
            tx.send(msg.clone())
                .await
                .or_else(|e| {
                    eprintln!("qos::retry_task could not send msg: {:?}", e);
                    Ok::<(), SendError<BrokerMsg>>(())
                })
                .unwrap();

            attempts += 1;
        }
    })
}
