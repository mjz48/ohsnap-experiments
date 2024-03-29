pub use config::Config;
pub use session::Session;

use crate::{
    error::{Error, Result},
    mqtt::{Publish, QoS, Subscribe, SubscribeTopic, Unsubscribe},
};
use client_handler::ClientHandler;
use log::{debug, error, info, trace};
use msg::BrokerMsg;
use std::collections::{hash_map::Entry, HashMap, HashSet};
use tokio::{
    net::TcpListener,
    sync::mpsc::{self, Receiver, Sender},
};

pub mod client_handler;
pub mod config;
pub mod msg;
pub mod session;

/// reserved capacity for client handler -> broker shared state channel
const BROKER_MSG_CHANNEL_CAPACITY: usize = 100;

/// Client individual informtion bookkeeping.
#[derive(Debug)]
pub struct ClientInfo {
    /// channel to send messages to this client
    pub client_tx: Sender<BrokerMsg>,
    /// list of topics that this client has subscribed to
    pub topics: HashSet<SubscribeTopic>,
}

/// The MQTT Broker contains shared state and external interface.
pub struct Broker {
    /// configuration object
    config: Config,
    /// mpsc channel to receive broker messages from all client handlers
    msg: (Sender<BrokerMsg>, Receiver<BrokerMsg>),
    /// list of connect clients and client info
    clients: HashMap<String, ClientInfo>,
    /// list of active topic paths and all clients subscribed to each
    subscriptions: HashMap<String, HashSet<String>>,
}

impl Broker {
    /// External Broker interface. Initializes the shared state and then
    /// starts the broker processes.
    ///
    /// # Examples
    ///
    /// ```rust
    /// #[tokio::main]
    /// async fn main() {
    ///     let ip = std::net::IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0));
    ///     let port = 1883;
    ///     let max_retries = 3;
    ///     let retry_interval = 20; // seconds
    ///     let timeout_interval = 30; // seconds
    ///
    ///     let config = mqttrs_1::broker::Config::new(
    ///         ip, port, max_retries, retry_interval, timeout_interval);
    ///
    ///     // uncomment this to run the broker
    ///     //mqttrs_1::broker::Broker::run(config)
    ///     //    .await.expect("MQTT protocol error occurred.");
    /// }
    /// ```
    ///
    /// # Arguments
    ///
    /// * `config` - a broker::Config object to specify operational settings
    ///
    /// # Errors
    ///
    /// This function may throw the following errors:
    ///
    /// * Error::BrokerMsgSendFailure
    /// * Error::ClientHandlerInvalidState
    /// * Error::CreateClientTaskFailed
    /// * Error::EncodeFailed
    /// * Error::InvalidPacket
    /// * Error::LoggerInitFailed
    /// * Error::MQTTProtocolViolation
    /// * Error::PacketSendFailed
    /// * Error::PacketReceiveFailed
    /// * Error::TokioErr
    pub async fn run(config: Config) -> Result<()> {
        let mut broker = Broker {
            config,
            msg: mpsc::channel(BROKER_MSG_CHANNEL_CAPACITY),
            clients: HashMap::new(),
            subscriptions: HashMap::new(),
        };

        info!(
            "Starting MQTT broker on {}:{}...",
            broker.config.addr.ip(),
            broker.config.addr.port()
        );

        broker.listen_for_new_connections(
            TcpListener::bind(broker.config.addr)
                .await
                .or_else(|e| Err(Error::TokioErr(e)))?,
            broker.msg.0.clone(),
        )?;

        broker.listen_for_broker_msgs().await?;

        Ok(())
    }

    /// Spawn a new async task to listen to the tcp stream of specified
    /// address and port for new client connections. Once a new connection
    /// has been detected, the broker spawns another task to take care of the
    /// MQTT protocol for each individual client. All client tasks happen
    /// simultaneously to provide responsive service.
    ///
    /// # Arguments
    ///
    /// * `tcp` - TcpListener created by Broker::run() to listen for connections
    /// * `broker_tx` - channel sender given to all clients to shared state
    ///
    /// # Panics
    ///
    /// This function may panic if there is problem reading the tcp stream.
    /// This is not intentinoal. The behavior should be changed to return
    /// TokioErr to the caller function for the messages to be handled at the
    /// external facing broker interface.
    fn listen_for_new_connections(
        &self,
        tcp: TcpListener,
        broker_tx: Sender<BrokerMsg>,
    ) -> Result<()> {
        let config = self.config.clone();

        tokio::spawn(async move {
            loop {
                // TODO: need to handle connection failure
                let (stream, addr) = tcp.accept().await.unwrap();
                debug!("New TCP connection detected: addr = {}", addr);

                let broker_tx = broker_tx.clone();
                let config = config.clone();

                tokio::spawn(async move {
                    trace!("Spawning new client task...");

                    match ClientHandler::run(config, stream, broker_tx).await {
                        Ok(()) => (),
                        Err(err) => {
                            error!("Error during client operation: {:?}", err);
                        }
                    }

                    trace!("Client task exiting...");
                });
            }
        });

        Ok(())
    }

    /// Listens for messages coming from client tasks that require action from
    /// the shared state.
    ///
    /// # Errors
    ///
    /// This function may throw the following errors:
    ///
    /// * Error::BrokerMsgsendFailure
    async fn listen_for_broker_msgs(&mut self) -> Result<()> {
        while let Some(msg) = self.msg.1.recv().await {
            trace!("Received BrokerMsg: {:?}", msg);

            match msg {
                BrokerMsg::ClientConnected { client, client_tx } => {
                    self.handle_client_connected(client, client_tx).await?;
                }
                BrokerMsg::ClientDisconnected { client } => {
                    self.handle_client_disconnected(client).await?;
                }
                BrokerMsg::Publish { client, packet } => {
                    self.handle_publish(client, packet).await?;
                }
                BrokerMsg::Subscribe { client, packet } => {
                    self.handle_subscribe(client, packet).await?;
                }
                BrokerMsg::Unsubscribe { client, packet } => {
                    self.handle_unsubscribe(client, packet).await?;
                }
                pkt => {
                    return Err(Error::InvalidPacket(format!(
                        "Unexpected broker msg received from client: {:?}",
                        pkt
                    )));
                }
            }
        }

        Ok(())
    }

    /// Shared state business logic for new client connections. The broker
    /// creates an entry if none exist or performs a session takeover if one
    /// does exist.
    ///
    /// # Arguments
    ///
    /// * `client` - client identifier string for indexing
    /// * `client_tx` - the client handler task channel for broker to handler comm.
    async fn handle_client_connected(
        &mut self,
        client: String,
        client_tx: Sender<BrokerMsg>,
    ) -> Result<()> {
        // TODO: implement session takeover if there are collisions
        self.clients.insert(
            client.clone(),
            ClientInfo {
                client_tx,
                topics: HashSet::new(),
            },
        );
        debug!("Broker state: {:?}", self.clients);
        Ok(())
    }

    /// Shared state business logic for client disconnections. This will be run
    /// both for when the broker disconnects and when the client disconnects.
    /// If an error occurs anywhere in broker or client handler logic, the
    /// client will usually be disconnected and this function will run.
    ///
    /// The broker will remove all traces of the client from the
    /// subscriptions and client list.
    ///
    /// NOTE: this should be re-examined when session expiry is implemented.
    /// We may not necessarily need to delete this stuff on disconnect if the
    /// session has not expired.
    ///
    /// # Arguments
    ///
    /// * `client` - client identifier of disconnected client
    async fn handle_client_disconnected(&mut self, client: String) -> Result<()> {
        // TODO: whether or not to remove client session info
        // actually depends on session expiry settings. Change
        // this to reflect that instead of always removing.
        if let Some(ref client_info) = self.clients.get(&client) {
            // remove client from all subscriptions
            for topic in client_info.topics.iter() {
                self.subscriptions
                    .entry(topic.topic_path.to_string())
                    .and_modify(|subs| {
                        subs.remove(&client);
                    });
            }
        }
        self.clients.remove(&client);

        debug!("Broker state: {:?}", self.clients);
        Ok(())
    }

    /// Shared broker state business logic for handling client publish requests.
    /// Upon receiving a publish MQTT control packet from a client handler,
    /// the broker should forward this message to all subscribers with
    /// matching topics.
    ///
    /// # Arguments
    ///
    /// * `client` - client identifier of the original sender
    /// * `dup` - set to true if this is a resend (QoS > 0), or false otherwise
    /// * `retain` - set to true if broker should retain message
    /// * `topic_name` - topic_name to publish this message
    /// * `payload` - data to transmit to subscribed clients
    ///
    /// # Errors
    ///
    /// This function may throw the following errors:
    ///
    /// * Error::BrokerMsgSendFailure
    /// * Error::MQTTProtocolViolation
    ///
    async fn handle_publish(&mut self, client: String, packet: Publish) -> Result<()> {
        if let Ok(ref payload_str) = String::from_utf8(packet.payload.to_vec()) {
            trace!("BrokerMsg::Publish payload string: {}", payload_str);
        }

        for client_id in self
            .subscriptions
            .entry(packet.topic_name.clone())
            .or_insert(HashSet::new())
            .iter()
        {
            let msg = BrokerMsg::Publish {
                client: client.clone(),
                packet: packet.clone(),
            };
            if let Some(client_info) = self.clients.get(client_id) {
                // don't resend this message to the original sender
                if *client_id == client {
                    continue;
                }

                trace!("Publishing message to {}, msg = {:?}", client_id, msg);
                client_info.client_tx.send(msg).await.or_else(|e| {
                    Err(Error::BrokerMsgSendFailure(format!(
                        "Could not send BrokerMsg: {:?}",
                        e
                    )))
                })?;
            }
        }
        Ok(())
    }

    /// Shared broker state for handling client subscribe requests. Upon
    /// receiving a subscribe MQTT control packet from the client, the broker
    /// should update internal state by adding the client to a list of
    /// subscribers for the given topics. Subsequent publishes will use the
    /// new state to route messages.
    ///
    /// # Arguments
    ///
    /// * `client` - client identifier of original sender
    /// * `pid` - pid of transaction
    /// * `topics` - a vector of topic paths to subscribe
    ///
    /// # Errors
    ///
    /// This function may throw the following errors:
    ///
    /// * Error::MQTTProtocolViolation
    async fn handle_subscribe(&mut self, client: String, packet: Subscribe) -> Result<()> {
        trace!(
            "BrokerMsg::Subscribe received! {{ client = {}, topics = {:?} }}",
            client,
            packet.topics
        );

        // TODO: validate client and topics string contents

        for topic in packet.topics.iter() {
            self.subscriptions
                .entry(topic.topic_path.clone())
                .or_insert(HashSet::new())
                .insert(client.clone());
        }

        Ok(())
    }

    /// Shared broker state business logic for handling client unsubscribe
    /// requests. The broker should remove the sending client from subscriber
    /// lists and remove the provided topic from client info.
    ///
    /// # Arguments
    ///
    /// * `client` - client identifier of the request sender
    /// * `pid` - pid of transaction
    /// * `topics` - list of topic paths to unsubscribe
    ///
    /// # Errors
    ///
    /// This function may throw the following errors:
    ///
    /// * Error::MQTTProtocolViolation
    async fn handle_unsubscribe(&mut self, client: String, packet: Unsubscribe) -> Result<()> {
        trace!(
            "BrokerMsg::Unsubscribe received! {{ client = {}, topics = {:?} }}",
            client,
            packet.topics
        );

        // 1. remove clientinfo from all subscriptions
        for topic in &packet.topics {
            if let Entry::Occupied(ref mut entry) = self.subscriptions.entry(topic.clone()) {
                entry.get_mut().remove(&client);
            }

            if let Entry::Occupied(entry) = self.subscriptions.entry(topic.clone()) {
                if entry.get().is_empty() {
                    entry.remove_entry();
                }
            }
        }
        trace!("Broker subscriptions: {:?}", self.subscriptions);

        // 2. remove specified topics from client's subscribed topics list
        self.clients.entry(client.clone()).and_modify(|info| {
            for topic in &packet.topics {
                info.topics.remove(&SubscribeTopic {
                    topic_path: topic.to_string(),
                    qos: QoS::AtMostOnce,
                });
            }
            trace!("Client state: {:?}", info);
        });

        Ok(())
    }
}
