use crate::mqtt::keep_alive;
use mqttrs::{clone_packet, decode_slice, Packet};
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

const CONNECTION_THREAD_POLL_INTERVAL: u16 = 1; // in seconds

pub type TcpThreadJoinHandle = JoinHandle<Result<(), io::Error>>;

pub struct MqttPacketTx {
    pub pkt: Vec<u8>,
    pub keep_alive: bool,
}

pub struct TcpThreadContext {
    pub join_handle: TcpThreadJoinHandle,
    pub tcp_write_tx: mpsc::Sender<MqttPacketTx>,
}

pub struct MqttPacketRx {
    pub buf: Vec<u8>,
}

#[derive(Debug)]
pub struct NotConnectedError;

impl Display for NotConnectedError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "no open tcp connection to listen on")
    }
}

impl Error for NotConnectedError {}

/// get a TcpStream and read existing data into a new buffer
// TODO: make private?
pub fn read_into_buf(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut reader = BufReader::new(stream);
    let mut received: Vec<u8> = reader.fill_buf()?.to_vec();
    reader.consume(received.len());

    let mut ret_buf: Vec<u8> = received.clone();
    clone_packet(&mut received as &mut [u8], &mut ret_buf as &mut [u8])?;
    return Ok(ret_buf);
}

/// get a TcpStream and a buffer and decode the data into an mqttrs::Packet
// TODO: make private?
pub fn read_and_decode<'a>(stream: &mut TcpStream, buf: &'a mut Vec<u8>) -> io::Result<Packet<'a>> {
    let mut reader = BufReader::new(stream);
    *buf = reader.fill_buf()?.to_vec();
    reader.consume(buf.len());

    match decode_slice(buf)? {
        Some(pkt) => Ok(pkt),
        None => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unable to decode incoming data",
        )),
    }
}

pub fn decode_tcp_rx<'a>(pkt: &'a MqttPacketRx) -> std::io::Result<mqttrs::Packet<'a>> {
    match mqttrs::decode_slice(&pkt.buf as &[u8])? {
        Some(pkt) => Ok(pkt),
        None => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "unable to decode incoming data",
        )),
    }
}

/// spawn a thread to manage tcp stream sending and receiving for the entire application
pub fn spawn_tcp_thread<'a>(
    hostname: &str,
    port: u16,
    keep_alive_tx: Option<mpsc::Sender<keep_alive::Msg>>,
    tcp_read_tx: mpsc::Sender<MqttPacketRx>,
) -> Result<TcpThreadContext, Box<dyn Error>> {
    let mut stream = TcpStream::connect(format!("{}:{}", hostname, port))?;
    let (tcp_write_tx, tcp_write_rx) = mpsc::channel();

    let join_handle = thread::spawn(move || -> Result<(), io::Error> {
        let old_stream_timeout = stream.read_timeout()?;
        stream.set_read_timeout(Some(Duration::from_secs(
            CONNECTION_THREAD_POLL_INTERVAL.into(),
        )))?;

        loop {
            // check for messages that need to be sent out over the tcp stream
            let mut tcp_write_rx_iter = tcp_write_rx.try_iter();
            let mut send_msg = tcp_write_rx_iter.next();

            while send_msg.is_some() {
                let msg: MqttPacketTx = send_msg.unwrap();

                stream.write(&msg.pkt)?;
                if msg.keep_alive {
                    if let Some(ref tx) = keep_alive_tx {
                        tx.send(keep_alive::Msg::Reset)
                            .or_else(|err| Err(io::Error::new(io::ErrorKind::NotConnected, err)))?;
                    }
                }

                send_msg = tcp_write_rx_iter.next();
            }

            match read_into_buf(&mut stream) {
                Ok(buf) => {
                    tcp_read_tx
                        .send(MqttPacketRx { buf })
                        .or_else(|err| Err(io::Error::new(io::ErrorKind::NotConnected, err)))?;
                }
                Err(err)
                    if err.kind() == io::ErrorKind::TimedOut
                        || err.kind() == io::ErrorKind::WouldBlock =>
                {
                    // silently ignore timeouts (we want to poll stream)
                    ()
                }
                Err(err) => {
                    stream
                        .set_read_timeout(old_stream_timeout)
                        .expect("unable to set read_timeout");
                    return Err(err);
                }
            }
        }
    });

    Ok(TcpThreadContext {
        join_handle,
        tcp_write_tx,
    })
}
