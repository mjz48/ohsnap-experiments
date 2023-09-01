use crate::cli::spec;
use crate::mqtt::MqttContext;
use signal_hook::{consts::SIGINT, consts::SIGTERM, iterator::Signals};
use std::{time::Duration, sync::mpsc::RecvTimeoutError};
use mqttrs::Packet;
use crate::tcp::{self, PacketTx, MqttPacketTx};
use std::sync::mpsc;

const POLL_INTERVAL: u64 = 1; // seconds

#[derive(Debug, Clone)]
struct MQTTError(String);

impl std::error::Error for MQTTError {}

impl std::fmt::Display for MQTTError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Error: unknown command {}", self.0)
    }
}

pub fn subscribe() -> spec::Command<MqttContext> {
    spec::Command::<MqttContext>::build("subscribe")
        .set_description("Subscribe to one or more topics from broker; Blocks the console until Ctrl-c is pressed")
        .set_usage("{$name} topic1 [topic2 ...]")
        .set_enable(|_command, _shell, _state, context: &mut MqttContext| {
            context.tcp_write_tx.is_some()
        })
        .set_callback(|command, _shell, _state, context| {
            let mut signals = Signals::new(&[SIGINT, SIGTERM])?;

            if command.operands().len() == 0 {
                return Err("Must provide at least one topic path to subscribe to.".into());
            }

            let mut topics = vec![];
            let mut topics_size = 0;
            for op in command.operands().iter() {
                topics.push(mqttrs::SubscribeTopic {
                    topic_path: op.value().to_owned(),
                    qos: mqttrs::QoS::AtMostOnce,
                });

                topics_size += op.value().len();
            }

            // TODO: implement pid handling
            let pid = mqttrs::Pid::new();
            let pkt = Packet::Subscribe(mqttrs::Subscribe {
                pid,
                topics,
            });

            // looks like you can generate a buf with dynamic size. This is
            // good because max packet size is 256MB and we don't want to make
            // something that big every time we publish anything.
            let buf_sz = std::mem::size_of::<mqttrs::Subscribe>() + topics_size;
            let mut buf = vec![0u8; buf_sz];
            mqttrs::encode_slice(&pkt, &mut buf)?;

            if let Err(err) = context.tcp_send(PacketTx::Mqtt(MqttPacketTx {
                pkt: buf,
                keep_alive: true,
            })) {
                match err {
                    mpsc::SendError(_) => {
                        return Err("Cannot subscribe without being connected to broker.".into());
                    }
                }
            }

            println!("Waiting on messages for subscribed topics...");
            println!("To exit, press Ctrl-c.");
        
            'subscribe: loop {
                // poll on signal_hook to check for Ctrl+c pressed
                for sig in signals.pending() {
                    match sig {
                        SIGTERM | SIGINT => {
                            println!("Subscribe cancelled.");
                            break 'subscribe;
                        }
                        _ => {
                            eprintln!("Received unexpected signal {:?}", sig);
                            break;
                        }
                    }
                }

                // check on publish messages
                let rx_pkt = context
                    .tcp_recv_timeout(Duration::from_secs(POLL_INTERVAL));

                if let Err(RecvTimeoutError::Timeout) = rx_pkt {
                    // timed out waiting, ignore error and run another poll loop iteration
                    continue;
                }

                let rx_pkt = if let Ok(pkt) = rx_pkt {
                    pkt
                } else {
                    // rx_pkt is None, meaning the channel has disconnected
                    // just exit, no other action required
                    break 'subscribe;
                };

                match tcp::decode_tcp_rx(&rx_pkt) {
                    Ok(Packet::Publish(publish)) => {
                        match std::str::from_utf8(publish.payload) {
                            Ok(msg) => { println!("['{}']: {}", publish.topic_name, msg) },
                            Err(_) => { println!("payload data: {:?}", publish.payload) },
                        }
                    },
                    Ok(Packet::Suback(suback)) => {
                        if pid != suback.pid {
                            eprintln!("Received Suback packet but Pid doesn't match our subscription request! {:?}", suback);
                            return Err(Box::new(
                                MQTTError(
                                    String::from("MQTT Suback protocol violation detected.")
                                )
                            ));
                        }
                        
                        // TODO: need to validate subscribe topics when QoS
                        // is implemented.

                        println!("Received expected suback from server.");
                    },
                    Err(err) => {
                        eprintln!("unable to decode mqtt packet: {:?}", err);
                        continue;
                    },
                    pkt => {
                        eprintln!("Received unexpected packet: {:?}", pkt);
                        continue;
                    }
                }
            }

            Ok(spec::ReturnCode::Ok)
        })
}
