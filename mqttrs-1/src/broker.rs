pub use self::config::Config;

use self::msg::BrokerMsg;
use crate::error::{Error, Result};
use client_handler::ClientHandler;
use log::{debug, error, info, trace};
use simplelog::{
    ColorChoice, CombinedLogger, Config as SLConfig, TermLogger, TerminalMode, WriteLogger,
};
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use tokio::net::TcpListener;
use tokio::sync::mpsc::{self, Receiver, Sender};

pub mod client_handler;
pub mod config;
pub mod msg;

const BROKER_MSG_CHANNEL_CAPACITY: usize = 100;

#[derive(Debug)]
pub struct ClientInfo {
    pub client_tx: Sender<BrokerMsg>,
    pub topics: HashSet<String>,
}

pub struct Broker {
    config: Config,
    msg: (Sender<BrokerMsg>, Receiver<BrokerMsg>),
    clients: HashMap<String, ClientInfo>,
    subscriptions: HashMap<String, HashSet<String>>,
}

impl Broker {
    pub async fn run(config: Config) -> Result<()> {
        Self::init_logger(&config)?;

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

        Self::listen_for_new_connections(
            TcpListener::bind(broker.config.addr)
                .await
                .or_else(|e| Err(Error::TokioErr(e)))?,
            broker.msg.0.clone(),
        )?;

        broker.listen_for_broker_msgs().await?;

        Ok(())
    }

    fn init_logger(config: &Config) -> Result<()> {
        // TODO: should this go in main.rs and be injected into Broker::new?
        // Should the simplelog be wrapped by an internal logging API?
        let level_filter = config.log_level;
        let log_config = SLConfig::default();

        let term_logger = TermLogger::new(
            level_filter,
            log_config.clone(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        );

        let log_dir = "log";
        let log_path = format!("{}/{}", log_dir, "broker.log");

        fs::create_dir_all(log_dir).or_else(|e| {
            Err(Error::LoggerInitFailed(format!(
                "Could not create log directories '{}': {:?}",
                log_dir, e
            )))
        })?;

        let log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
            .or_else(|e| {
                Err(Error::LoggerInitFailed(format!(
                    "Could not create log file for WriteLogger: {:?}",
                    e
                )))
            })?;

        let write_logger = WriteLogger::new(level_filter, log_config, log_file);

        CombinedLogger::init(vec![term_logger, write_logger]).or_else(|e| {
            Err(Error::LoggerInitFailed(format!(
                "Logger init failed: {:?}",
                e
            )))
        })?;

        Ok(())
    }

    fn listen_for_new_connections(tcp: TcpListener, broker_tx: Sender<BrokerMsg>) -> Result<()> {
        tokio::spawn(async move {
            loop {
                // TODO: need to handle connection failure
                let (stream, addr) = tcp.accept().await.unwrap();
                debug!("New TCP connection detected: addr = {}", addr);

                let broker_tx = broker_tx.clone();

                tokio::spawn(async move {
                    trace!("Spawning new client task...");

                    match ClientHandler::run(stream, broker_tx).await {
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
                BrokerMsg::Publish {
                    client,
                    dup,
                    retain,
                    topic_name,
                    payload,
                } => {
                    self.handle_publish(client, dup, retain, topic_name, payload)
                        .await?;
                }
                BrokerMsg::Subscribe { client, topics } => {
                    self.handle_subscribe(client, topics).await?;
                }
                BrokerMsg::Unsubscribe { client, topics } => {
                    self.handle_unsubscribe(client, topics).await?;
                }
            }
        }

        Ok(())
    }

    async fn handle_client_connected(
        &mut self,
        client: String,
        client_tx: Sender<BrokerMsg>,
    ) -> Result<()> {
        self.clients.insert(
            client,
            ClientInfo {
                client_tx,
                topics: HashSet::new(),
            },
        );
        debug!("Broker state: {:?}", self.clients);
        Ok(())
    }

    async fn handle_client_disconnected(&mut self, client: String) -> Result<()> {
        // TODO: whether or not to remove client session info
        // actually depends on session expiry settings. Change
        // this to reflect that instead of always removing.
        if let Some(ref client_info) = self.clients.get(&client) {
            // remove client from all subscriptions
            for topic in client_info.topics.iter() {
                self.subscriptions
                    .entry(String::from(topic))
                    .and_modify(|subs| {
                        subs.remove(&client);
                    });
            }
        }
        self.clients.remove(&client);

        debug!("Broker state: {:?}", self.clients);
        Ok(())
    }

    async fn handle_publish(
        &mut self,
        client: String,
        dup: bool,
        retain: bool,
        topic_name: String,
        payload: Vec<u8>,
    ) -> Result<()> {
        if let Ok(ref payload_str) = String::from_utf8(payload.to_vec()) {
            trace!("BrokerMsg::Publish payload string: {}", payload_str);
        }

        for client_id in self
            .subscriptions
            .entry(topic_name.clone())
            .or_insert(HashSet::new())
            .iter()
        {
            let msg = BrokerMsg::Publish {
                client: client.clone(),
                dup,
                retain,
                topic_name: topic_name.clone(),
                payload: payload.clone(),
            };

            if let Some(client_info) = self.clients.get(client_id) {
                // don't resend this message to the original sender
                if *client_id == client {
                    continue;
                }

                trace!("Publishing message to {}", client_id);
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

    async fn handle_subscribe(&mut self, client: String, topics: Vec<String>) -> Result<()> {
        trace!(
            "BrokerMsg::Subscribe received! {{ client = {}, topics = {:?} }}",
            client,
            topics
        );

        // TODO: validate client and topics string contents

        for topic in topics.iter() {
            self.subscriptions
                .entry(topic.clone())
                .or_insert(HashSet::new())
                .insert(client.clone());
        }

        Ok(())
    }

    async fn handle_unsubscribe(&mut self, client: String, topics: Vec<String>) -> Result<()> {
        trace!(
            "BrokerMsg::Unsubscribe received! {{ client = {}, topics = {:?} }}",
            client,
            topics
        );

        // 1. remove clientinfo from all subscriptions
        for topic in &topics {
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
            for topic in &topics {
                info.topics.remove(topic);
            }
            trace!("Client state: {:?}", info);
        });

        Ok(())
    }
}
