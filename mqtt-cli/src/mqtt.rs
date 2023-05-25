use crate::tcp::{PacketRx, PacketTx};
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
    pub connection: Option<TcpStream>, // TODO: delete

    pub keep_alive: Option<(JoinHandle<()>, mpsc::Sender<keep_alive::Msg>)>, // TODO: delete

    pub keep_alive_tx: Option<mpsc::Sender<keep_alive::Msg>>,

    pub tcp_write_tx: Option<mpsc::Sender<PacketTx>>,
    pub tcp_read_rx: Option<mpsc::Receiver<PacketRx>>,
}

impl MqttContext {
    pub fn tcp_send(&self, pkt: PacketTx) -> Result<(), mpsc::SendError<PacketTx>> {
        match self.tcp_write_tx {
            Some(ref tx) => {
                tx.send(pkt)?;
                Ok(())
            }
            None => Err(mpsc::SendError(pkt)),
        }
    }

    pub fn tcp_recv(&mut self) -> Result<PacketRx, mpsc::RecvError> {
        match self.tcp_read_rx {
            Some(ref rx) => Ok(rx.recv()?),
            None => Err(mpsc::RecvError),
        }
    }

    pub fn tcp_recv_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<PacketRx, mpsc::RecvTimeoutError> {
        match self.tcp_read_rx {
            Some(ref rx) => Ok(rx.recv_timeout(timeout)?),
            None => Err(mpsc::RecvTimeoutError::Disconnected),
        }
    }
}
