use crate::error::{Error, Result};
use client_handler::ClientHandler;
use log::{debug, error, info, trace};
use simplelog::{
    ColorChoice, CombinedLogger, Config as SLConfig, LevelFilter, TermLogger, TerminalMode,
    WriteLogger,
};
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::net::{IpAddr, SocketAddr};
use tokio::net::TcpListener;
use tokio::sync::mpsc::{self, Receiver, Sender};

pub mod client_handler;

const BROKER_MSG_CHANNEL_CAPACITY: usize = 100;

pub struct Config {
    addr: SocketAddr,
}

impl Config {
    pub fn new(ip: IpAddr, port: u16) -> Config {
        Config {
            addr: SocketAddr::new(ip, port),
        }
    }
}

#[derive(Debug)]
pub enum BrokerMsg {
    ClientConnected {
        client: String,
        client_tx: Sender<BrokerMsg>,
    },
    ClientDisconnected {
        client: String,
    },
    Publish {
        client: String,
        // these fields are from mqttrs::Publish
        dup: bool,
        //qospid: QosPid // TODO: implement
        retain: bool,
        topic_name: String,
        payload: Vec<u8>,
    },
    Subscribe {
        client: String,
        // these fields are from mqttrs::Subscribe
        //pid: Pid // TODO: implement
        topics: Vec<String>, // TODO: this is originally SubscribeTopics type, which has QoS informatino
    },
}

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
        // TODO: should this go in main.rs and be injected into Broker::new?
        // Should the simplelog be wrapped by an internal logging API?
        {
            let level_filter = LevelFilter::Trace;
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
        }

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

        let tcp = TcpListener::bind(broker.config.addr)
            .await
            .or_else(|e| Err(Error::TokioErr(e)))?;

        let broker_tx = broker.msg.0.clone();
        let broker_rx = &mut broker.msg.1;

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

        while let Some(msg) = broker_rx.recv().await {
            trace!("Received BrokerMsg: {:?}", msg);

            match msg {
                BrokerMsg::ClientConnected { client, client_tx } => {
                    broker.clients.insert(
                        client,
                        ClientInfo {
                            client_tx,
                            topics: HashSet::new(),
                        },
                    );
                    debug!("Broker state: {:?}", broker.clients);
                }
                BrokerMsg::ClientDisconnected { client } => {
                    // TODO: whether or not to remove client session info
                    // actually depends on session expiry settings. Change
                    // this to reflect that instead of always removing.
                    if let Some(ref client_info) = broker.clients.get(&client) {
                        // remove client from all subscriptions
                        for topic in client_info.topics.iter() {
                            broker
                                .subscriptions
                                .entry(String::from(topic))
                                .and_modify(|subs| {
                                    subs.remove(&client);
                                });
                        }
                    }
                    broker.clients.remove(&client);

                    debug!("Broker state: {:?}", broker.clients);
                }
                BrokerMsg::Publish {
                    client,
                    dup,
                    retain,
                    topic_name,
                    payload,
                } => {
                    if let Ok(ref payload_str) = String::from_utf8(payload.to_vec()) {
                        trace!("BrokerMsg::Publish payload string: {}", payload_str);
                    }

                    for client_id in broker
                        .subscriptions
                        .entry(topic_name.clone())
                        .or_insert(HashSet::new())
                        .iter()
                    {
                        let msg = BrokerMsg::Publish {
                            client: "".into(), // should we leave this empty for Broker to Client publish messages?
                            dup,
                            retain,
                            topic_name: topic_name.clone(),
                            payload: payload.clone(),
                        };

                        if let Some(client_info) = broker.clients.get(client_id) {
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
                }
                BrokerMsg::Subscribe { client, topics } => {
                    trace!(
                        "BrokerMsg::Subscribe received! {{ client = {}, topics = {:?} }}",
                        client,
                        topics
                    );

                    // TODO: validate client and topics string contents

                    for topic in topics.iter() {
                        broker
                            .subscriptions
                            .entry(topic.clone())
                            .or_insert(HashSet::new())
                            .insert(client.clone());
                    }
                }
            }
        }

        Ok(())
    }
}
