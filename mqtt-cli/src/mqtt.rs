use crate::tcp_stream::{MqttPacketRx, MqttPacketTx, TcpThreadJoinHandle};
use std::net::TcpStream;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

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
    pub username: Option<String>,

    pub broker: BrokerAddr,
    pub connection: Option<TcpStream>,

    pub keep_alive: Option<(JoinHandle<()>, mpsc::Sender<keep_alive::Msg>)>,

    pub keep_alive_thread: Option<JoinHandle<()>>,
    pub keep_alive_tx: Option<mpsc::Sender<keep_alive::Msg>>,

    pub connection_thread: Option<TcpThreadJoinHandle>,
    pub tcp_write_tx: Option<mpsc::Sender<MqttPacketTx>>,
    pub tcp_read_rx: Option<mpsc::Receiver<MqttPacketRx>>,
}

impl MqttContext {
    pub fn tcp_send(&self, pkt: MqttPacketTx) -> Result<(), mpsc::SendError<MqttPacketTx>> {
        match self.tcp_write_tx {
            Some(ref tx) => {
                tx.send(pkt)?;
                Ok(())
            }
            None => Err(mpsc::SendError(pkt)),
        }
    }

    pub fn tcp_recv(&mut self) -> Result<MqttPacketRx, mpsc::RecvError> {
        match self.tcp_read_rx {
            Some(ref rx) => Ok(rx.recv()?),
            None => Err(mpsc::RecvError),
        }
    }

    pub fn tcp_recv_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<MqttPacketRx, mpsc::RecvTimeoutError> {
        match self.tcp_read_rx {
            Some(ref rx) => Ok(rx.recv_timeout(timeout)?),
            None => Err(mpsc::RecvTimeoutError::Disconnected),
        }
    }
}
