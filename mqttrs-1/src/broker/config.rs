use simplelog::LevelFilter;
use std::net::{IpAddr, SocketAddr};

/// Broker configuration object. Inject this into Broker::run.
#[derive(Debug, Copy, Clone)]
pub struct Config {
    /// Ip address for the broker to listen on
    pub addr: SocketAddr,
    /// log verbosity (log will output all levels higher than specified)
    /// Log filter levels: Off, Error, Warn, Info, Debug, Trace
    pub log_level: LevelFilter,
    /// time to wait before re-sending QoS>0 packets (in seconds)
    pub retry_interval: u32,
}

impl Config {
    /// Create a new config object
    pub fn new(ip: IpAddr, port: u16, log_level: LevelFilter, retry_interval: u32) -> Config {
        Config {
            addr: SocketAddr::new(ip, port),
            log_level,
            retry_interval,
        }
    }
}
