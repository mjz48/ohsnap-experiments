use simplelog::LevelFilter;
use std::net::{IpAddr, SocketAddr};

/// Broker configuration object. Inject this into Broker::run.
pub struct Config {
    /// Ip address for the broker to listen on
    pub addr: SocketAddr,
    /// log verbosity (log will output all levels higher than specified)
    /// Log filter levels: Off, Error, Warn, Info, Debug, Trace
    pub log_level: LevelFilter,
}

impl Config {
    /// Create a new config object
    pub fn new(ip: IpAddr, port: u16, log_level: LevelFilter) -> Config {
        Config {
            addr: SocketAddr::new(ip, port),
            log_level,
        }
    }
}
