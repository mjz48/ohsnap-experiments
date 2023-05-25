use crate::mqtt::keep_alive;
use mqttrs::clone_packet;
use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::mpsc;
use std::sync::mpsc::SendError;
use std::thread::{self, JoinHandle};
use std::time::Duration;

const CONNECTION_THREAD_POLL_INTERVAL: u64 = 100; // in milliseconds

pub type TcpThreadJoinHandle = JoinHandle<()>;

pub enum PacketTx {
    Mqtt(MqttPacketTx),
    Close,
}

pub struct MqttPacketTx {
    pub pkt: Vec<u8>,
    pub keep_alive: bool,
}

pub enum PacketRx {
    Mqtt(MqttPacketRx),
    Disconnected, // the broker has closed the connection
}

pub struct MqttPacketRx {
    pub buf: Vec<u8>,
}

pub struct TcpThreadContext {
    pub join_handle: TcpThreadJoinHandle,
    pub tcp_write_tx: mpsc::Sender<PacketTx>,
}

pub fn decode_tcp_rx<'a>(pkt: &'a PacketRx) -> io::Result<mqttrs::Packet<'a>> {
    match pkt {
        PacketRx::Mqtt(p) => match mqttrs::decode_slice(&p.buf as &[u8])? {
            Some(pkt) => Ok(pkt),
            None => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unable to decode incoming data",
            )),
        },
        PacketRx::Disconnected => Err(io::Error::new(
            io::ErrorKind::ConnectionAborted,
            "tcp stream closed.",
        )),
    }
}

/// spawn a thread to manage tcp stream sending and receiving for the entire application
pub fn spawn_tcp_thread(
    hostname: &str,
    port: u16,
    keep_alive_tx: Option<mpsc::Sender<keep_alive::Msg>>,
    tcp_read_tx: mpsc::Sender<PacketRx>,
    shell_cmd_tx: mpsc::Sender<String>,
) -> Result<TcpThreadContext, io::Error> {
    let mut stream = TcpStream::connect(format!("{}:{}", hostname, port))?;
    let (tcp_write_tx, tcp_write_rx) = mpsc::channel();

    let join_handle = thread::spawn(move || {
        let old_stream_timeout = stream
            .read_timeout()
            .unwrap_or(Some(Duration::from_secs(0)));

        stream
            .set_read_timeout(Some(Duration::from_millis(CONNECTION_THREAD_POLL_INTERVAL)))
            .expect("Unable to set read timeout on tcp stream.");

        'thread: loop {
            match read_into_buf(&mut stream) {
                Ok(buf) => {
                    if buf.len() == 0 {
                        // apparently, when the tcp stream is closed, a zero length packet is
                        // written to both sides of the stream. So if we are here, the stream has
                        // just been closed.
                        if let Err(SendError(_)) = shell_cmd_tx.send("disconnect -c".into()) {
                            eprintln!("tcp thread: unable to send to tcp_read_tx.");
                        }
                        break 'thread;
                    }

                    if let Err(SendError(_)) =
                        tcp_read_tx.send(PacketRx::Mqtt(MqttPacketRx { buf }))
                    {
                        eprintln!("tcp thread: unable to send to tcp_read_tx.");
                        break 'thread;
                    }
                }
                Err(err)
                    if err.kind() == io::ErrorKind::TimedOut
                        || err.kind() == io::ErrorKind::WouldBlock =>
                {
                    // silently ignore timeouts (we want to poll stream)
                    ()
                }
                Err(err) => {
                    eprintln!("tcp thread: error reading tcp stream: {:?}", err);
                    break 'thread;
                }
            }

            // check for messages that need to be sent out over the tcp stream
            let mut tcp_write_rx_iter = tcp_write_rx.try_iter();
            let mut send_msg = tcp_write_rx_iter.next();

            while send_msg.is_some() {
                let msg: PacketTx = send_msg.unwrap();
                match msg {
                    PacketTx::Mqtt(mqtt_pkt) => {
                        if let Err(err) = stream.write(&mqtt_pkt.pkt) {
                            eprintln!("unable to send MqttPacketTx: {:?}", err);
                            break 'thread;
                        }
                        if mqtt_pkt.keep_alive {
                            if let Some(ref tx) = keep_alive_tx {
                                if let Err(SendError(_)) = tx.send(keep_alive::Msg::Reset) {
                                    eprintln!("tcp thread: unable to reset keep_alive timeout.");
                                }
                            }
                        }
                    }
                    PacketTx::Close => {
                        break 'thread;
                    }
                }

                send_msg = tcp_write_rx_iter.next();
            }
        }

        stream
            .set_read_timeout(old_stream_timeout)
            .expect("unable to set read_timeout");
    });

    Ok(TcpThreadContext {
        join_handle,
        tcp_write_tx,
    })
}

/// get a TcpStream and read existing data into a new buffer
fn read_into_buf(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut reader = BufReader::new(stream);
    let mut received: Vec<u8> = reader.fill_buf()?.to_vec();
    reader.consume(received.len());

    let mut ret_buf: Vec<u8> = received.clone();
    clone_packet(&mut received as &mut [u8], &mut ret_buf as &mut [u8])?;
    return Ok(ret_buf);
}
