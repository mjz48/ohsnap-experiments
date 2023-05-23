use crate::cli::spec;
use crate::mqtt::keep_alive::KeepAliveTcpStream;
use crate::mqtt::MqttContext;
use signal_hook::{consts::SIGINT, consts::SIGTERM, iterator::Signals};
use std::time::Duration;
use crate::tcp_stream;
use mqttrs::Packet;

const POLL_INTERVAL: u16 = 1; // seconds

pub fn subscribe() -> spec::Command<MqttContext> {
    spec::Command::<MqttContext>::build("subscribe")
        .set_help("Subscribe to one or more topics from broker. Blocks the console until Ctrl-c is pressed.")
        .set_callback(|command, _shell, _state, context| {
            let mut signals = Signals::new(&[SIGINT, SIGTERM])?;

            let stream = if let Some(ref mut tcp_stream) = context.connection {
                tcp_stream
            } else {
                return Err("cannot publish without established connection".into());
            };

            let keep_alive_tx = if let Some((_, ref tx)) = context.keep_alive {
                Some(tx.clone())
            } else {
                None
            };

            let mut topics = vec![];
            for op in command.operands().iter() {
                topics.push(mqttrs::SubscribeTopic {
                    topic_path: op.value().to_owned(),
                    qos: mqttrs::QoS::AtMostOnce,
                });
            }

            // TODO: implement pid handling
            let pkt = Packet::Subscribe(mqttrs::Subscribe {
                pid: mqttrs::Pid::new(),
                topics,
            });

            // looks like you can generate a buf with dynamic size. This is
            // good because max packet size is 256MB and we don't want to make
            // something that big every time we publish anything.
            let buf_sz = if let mqttrs::Packet::Subscribe(_) = pkt {
                std::mem::size_of::<mqttrs::Subscribe>()
            } else {
                0
            };
            let mut buf = vec![0u8; buf_sz];

            mqttrs::encode_slice(&pkt, &mut buf)?;
            (stream as &mut dyn KeepAliveTcpStream)
                .write(&buf, keep_alive_tx)
                .expect("Could not send subscribe message.");

            println!("Waiting on messages for the following topics:");
            println!("To exit, press Ctrl+c.");
        
            let old_stream_timeout = stream.read_timeout()?;
            stream.set_read_timeout(Some(Duration::from_secs(POLL_INTERVAL.into())))?;

            'subscribe: loop {
                // poll on signal_hook to check for Ctrl+c pressed
                for sig in signals.pending() {
                    match sig {
                        SIGTERM | SIGINT => {
                            println!("Subscribe cancelled.");
                            break 'subscribe;
                        }
                        _ => {
                            println!("Received unexpected signal {:?}", sig);
                            break;
                        }
                    }
                }

                // check on publish messages
                let mut ret_pkt_buf: Vec<u8> = Vec::new();
                match tcp_stream::read_and_decode(stream, &mut ret_pkt_buf) {
                    Ok(Packet::Publish(publish)) => {
                        match std::str::from_utf8(publish.payload) {
                            Ok(msg) => { println!("{}", msg) },
                            Err(_) => { println!("payload data: {:?}", publish.payload) },
                        }
                    },
                    Ok(pkt) => {
                        return Err(format!("Received unexpected packet during connection attempt:\n{:?}", pkt).into());
                    },
                    // timeout returns this in Windows
                    Err(err) if err.kind() == std::io::ErrorKind::TimedOut => {
                        () // ignore timeout silently
                    },
                    // timeout returns this in Unix/Linux
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        () // ignore timeout silently
                    },
                    Err(err) => {
                        return Err(format!("Problem with tcp connection: {:?}", err).into())
                    },
                }
            }
            stream.set_read_timeout(old_stream_timeout)?;

            Ok(spec::ReturnCode::Ok)
        })
}
