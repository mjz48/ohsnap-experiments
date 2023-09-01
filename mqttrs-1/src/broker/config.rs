use std::net::{IpAddr, SocketAddr};

pub struct Config {
    pub addr: SocketAddr,
}

impl Config {
    pub fn new(ip: IpAddr, port: u16) -> Config {
        Config {
            addr: SocketAddr::new(ip, port),
        }
    }
}
