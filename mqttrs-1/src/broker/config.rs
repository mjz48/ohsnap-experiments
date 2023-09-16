use std::net::{IpAddr, SocketAddr};

/// Broker configuration object. Inject this into Broker::run.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Config {
    /// Ip address for the broker to listen on
    pub addr: SocketAddr,
    /// maximum number of attempts to retry. 0 means infinite retries
    pub max_retries: u16,
    /// time to wait before re-sending QoS>0 packets (in seconds)
    pub retry_interval: u32,
    /// time to wait before taking error handling action (e.g. connection timeout)
    pub timeout_interval: u32,
}

impl Config {
    /// Create a new config object
    pub fn new(
        ip: IpAddr,
        port: u16,
        max_retries: u16,
        retry_interval: u32,
        timeout_interval: u32,
    ) -> Config {
        Config {
            addr: SocketAddr::new(ip, port),
            max_retries,
            retry_interval,
            timeout_interval,
        }
    }
}
