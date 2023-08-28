use crate::error::{LoggerInitFailedError, MQTTError};
use client_handler::ClientHandler;
use log::{error, info};
use simplelog::{
    ColorChoice, CombinedLogger, Config as SLConfig, LevelFilter, TermLogger, TerminalMode,
    WriteLogger,
};
use std::fs::{self, OpenOptions};
use std::net::{IpAddr, SocketAddr};
use tokio::net::TcpListener;

pub mod client_handler;

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

pub struct Broker {
    config: Config,
    tcp: Option<TcpListener>,
}

impl Broker {
    pub fn new(config: Config) -> Result<Broker, MQTTError> {
        // TODO: should this go in main.rs and be injected into Broker::new?
        // Should the simplelog wrap an internal logging API?
        {
            let level_filter = LevelFilter::Debug;
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
                Err(MQTTError::LoggerInit(LoggerInitFailedError(format!(
                    "{:?}",
                    e
                ))))
            })?;

            let log_file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_path)
                .or_else(|e| {
                    Err(MQTTError::LoggerInit(LoggerInitFailedError(format!(
                        "{:?}",
                        e
                    ))))
                })?;

            let write_logger = WriteLogger::new(level_filter, log_config, log_file);

            CombinedLogger::init(vec![term_logger, write_logger]).or_else(|e| {
                Err(MQTTError::LoggerInit(LoggerInitFailedError(format!(
                    "{:?}",
                    e
                ))))
            })?;
        }

        Ok(Broker { config, tcp: None })
    }

    pub async fn start(mut self) -> tokio::io::Result<()> {
        info!(
            "Starting MQTT broker on {}:{}...",
            self.config.addr.ip(),
            self.config.addr.port()
        );

        self.tcp = Some(TcpListener::bind(self.config.addr).await?);

        loop {
            // TODO: need to handle connection failure
            let (stream, _addr) = self.tcp.as_ref().unwrap().accept().await.unwrap();
            println!("New connection: {}", self.config.addr);

            tokio::spawn(async move {
                match ClientHandler::new(stream).run().await {
                    Ok(()) => (),
                    Err(err) => {
                        error!("Error during client operation: {:?}", err);
                    }
                }
            });
        }
    }
}
