use crate::cli::spec;
use crate::mqtt::MqttContext;
use signal_hook::{consts::SIGINT, consts::SIGTERM, iterator::Signals};
use std::{time::Duration, sync::mpsc::RecvTimeoutError};
use mqttrs::Packet;
use crate::tcp::{self, PacketTx, MqttPacketTx};
use std::sync::mpsc;

const POLL_INTERVAL: u64 = 1; // seconds

pub fn subscribe() -> spec::Command<MqttContext> {
    spec::Command::<MqttContext>::build("subscribe")
        .set_help("Subscribe to one or more topics from broker. Blocks the console until Ctrl-c is pressed.")
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
            let pkt = Packet::Subscribe(mqttrs::Subscribe {
                pid: mqttrs::Pid::new(),
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

            println!("Waiting on messages for the following topics:");
            println!("To exit, press Ctrl+c.");
        
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
                let rx_pkt = rx_pkt.unwrap();

                let mqtt_pkt = tcp::decode_tcp_rx(&rx_pkt);
                if let Err(err) = mqtt_pkt {
                    eprintln!("unable to decode mqtt packet: {:?}", err);
                    continue;
                }

                match mqtt_pkt.unwrap() {
                    Packet::Publish(publish) => {
                        match std::str::from_utf8(publish.payload) {
                            Ok(msg) => { println!("{}", msg) },
                            Err(_) => { println!("payload data: {:?}", publish.payload) },
                        }
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
