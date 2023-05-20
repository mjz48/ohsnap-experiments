use std::net::TcpStream;
use std::sync::mpsc;
use std::thread::JoinHandle;

pub mod keep_alive;

// tcp socket
pub type Port = u16;

#[derive(Debug)]
pub struct BrokerAddr {
    pub hostname: String,
    pub port: Port,
}

impl BrokerAddr {
    pub fn new() -> BrokerAddr {
        BrokerAddr {
            hostname: "127.0.0.1".into(),
            port: 1883,
        }
    }
}

// shared data structure to pass information between cli commands
#[derive(Debug)]
pub struct MqttContext {
    pub prompt_string: String,

    pub client_id: String,
    pub broker: BrokerAddr,
    pub connection: Option<TcpStream>,

    pub keep_alive: Option<(JoinHandle<()>, mpsc::Sender<keep_alive::WakeReason>)>,
}
