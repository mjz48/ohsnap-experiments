use std::net::TcpStream;

// tcp socket
pub type Port = u16;

pub struct BrokerAddr {
    pub hostname: String,
    pub port: Port,
}

impl BrokerAddr {
    pub fn new() -> BrokerAddr {
        BrokerAddr { hostname: "127.0.0.1".into(), port: 1883 }
    }
}

// shared data structure to pass information between cli commands
pub struct MqttContext {
    pub prompt_string: String,

    pub client_id: String,
    pub broker: BrokerAddr,
    pub connection: Option<TcpStream>,
}
