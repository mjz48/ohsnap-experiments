use simplelog::LevelFilter;
use std::net::{IpAddr, SocketAddr};

pub struct Config {
    pub addr: SocketAddr,
    pub log_level: LevelFilter,
}

impl Config {
    pub fn new(ip: IpAddr, port: u16, log_level: LevelFilter) -> Config {
        Config {
            addr: SocketAddr::new(ip, port),
            log_level,
        }
    }
}
