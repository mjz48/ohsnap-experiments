use crate::cli::spec;
use crate::mqtt::MqttContext;
use signal_hook::{consts::SIGINT, consts::SIGTERM, iterator::Signals};
use std::{time::Duration, sync::mpsc::{RecvTimeoutError, SendError}};
use std::error::Error;
use mqttrs::Packet;
use crate::tcp::{self, PacketRx, PacketTx, MqttPacketTx};
use std::sync::mpsc;

const POLL_INTERVAL: u64 = 1; // seconds
const UNSUBACK_TIMEOUT: u64 = 30; // seconds
const MAX_RETRIES: usize = 5;
const RETRY_TIMEOUT: u64 = 20; // in seconds

#[derive(Debug, Clone)]
struct MQTTError(String);

impl std::error::Error for MQTTError {}

impl std::fmt::Display for MQTTError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Error: unknown command {}", self.0)
    }
}

#[derive(Debug)]
pub struct UnsubAckTimeoutError(RecvTimeoutError);

impl std::fmt::Display for UnsubAckTimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.0 {
            RecvTimeoutError::Timeout => {
                write!(f, "Timeout waiting for UNSUBACK.")
            }
            RecvTimeoutError::Disconnected => {
                write!(f, "Disconnected while waiting for UNSUBACK.")
            }
        }
    }
}
impl Error for UnsubAckTimeoutError {}

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
                pid: pid.clone(),
                topics: topics.clone(),
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

            // use this structure to keep track of subscription info
            let mut subscriptions = Vec::<mqttrs::SubscribeReturnCodes>::new();
        
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
                    return Ok(spec::ReturnCode::Ok);
                };

                match tcp::decode_tcp_rx(&rx_pkt) {
                    Ok(Packet::Publish(publish)) => {
                        match std::str::from_utf8(publish.payload) {
                            Ok(msg) => { println!("['{}']: {}", publish.topic_name, msg) },
                            Err(_) => { println!("payload data: {:?}", publish.payload) },
                        }

                        match publish.qospid {
                            mqttrs::QosPid::AtMostOnce => (),
                            mqttrs::QosPid::AtLeastOnce(pid) => {
                                // only need to respond with puback
                                let puback = Packet::Puback(pid);
                                let buf_sz = std::mem::size_of::<Packet>() + std::mem::size_of::<mqttrs::Pid>();
                                let mut buf = vec![0u8; buf_sz];
                                mqttrs::encode_slice(&puback, &mut buf)?;

                                if let Err(err) = context.tcp_send(PacketTx::Mqtt(MqttPacketTx {
                                    pkt: buf,
                                    keep_alive: true,
                                })) {
                                    match err {
                                        mpsc::SendError(_) => {
                                            return Err("Cannot puback without being connected to broker.".into());
                                        }
                                    }
                                }
                            }
                            mqttrs::QosPid::ExactlyOnce(ref pid) => {
                                let mut num_retries = 0;

                                // send pubrec (and wait for pubrel with retry)
                                let pubrec = Packet::Pubrec(pid.clone());
                                send_packet(&pubrec, context)?;
                                
                                while MAX_RETRIES - num_retries > 0 {
                                    let rx_pkt = match wait_on_response_with_retry(
                                        &pubrec, context, MAX_RETRIES - num_retries, RETRY_TIMEOUT
                                    )? {
                                        Some((pkt, attempts)) => {
                                            num_retries += attempts;
                                            pkt
                                        },
                                        None => {
                                            return Err(Box::new(MQTTError(
                                                "Timeout waiting on pubrel.".into(),
                                            )));
                                        }
                                    };

                                    // check if we got a pubrel with matching pid
                                    let resp_pid = match tcp::decode_tcp_rx(&rx_pkt)? {
                                        mqttrs::Packet::Pubrel(resp_pid) => { resp_pid },
                                        _ => { continue }
                                    };

                                    if &resp_pid != pid {
                                        continue;
                                    }

                                    println!("Received pubrel from server: Pubrel{{{:?}}}", resp_pid);
                                    break;
                                }

                                // send pubcomp and finish transaction
                                let pubcomp = mqttrs::Packet::Pubcomp(pid.clone());
                                send_packet(&pubcomp, context)?;

                                println!("Sending pubcomp to server and completing transaction: Pubcomp{{{:?}}}", pid);
                            }
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

                        for (idx, return_code) in suback.return_codes.iter().enumerate() {
                            subscriptions.push(*return_code);

                            if let mqttrs::SubscribeReturnCodes::Success(qos) = return_code {
                                if *qos != topics[idx].qos {
                                    println!(
                                        "WARNING: subscribe return code QoS for topic '{}' does not match: expected = {:?}, actual = {:?}",
                                        topics[idx].topic_path,
                                        topics[idx].qos,
                                        qos
                                    );
                                }
                            } else {
                                println!("WARNING: subscribe return code for topic '{}' returned Failure!",
                                    topics[idx].topic_path
                                );
                            }
                        }
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

            // once we've broken out of the subscribe loop, need to send
            // unsubscribe request to broker
            // 
            // NOTE: Originally wanted to use shell_cmd_tx queue to 
            // send an unsubscribe command, but the shell is blocked
            // until the current command (i.e. this subscribe command)
            // finishes. So manually send mqttrs packet here. Maybe
            // in the next revision, use async to unblock?
            let pkt = Packet::Unsubscribe(mqttrs::Unsubscribe {
                pid,
                topics: topics
                    .iter()
                    .map(|ref t| { t.topic_path.clone() })
                    .collect(),
            });
            let buf_sz = std::mem::size_of::<mqttrs::Unsubscribe>() + topics_size;
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

            // now wait for unsuback from broker
            let rx_pkt = context
                .tcp_recv_timeout(Duration::from_secs(UNSUBACK_TIMEOUT))
                .map_err(|err| UnsubAckTimeoutError(err))?;

            match tcp::decode_tcp_rx(&rx_pkt)? {
                Packet::Unsuback(unsub_pid) => {
                    if unsub_pid != pid {
                        // actually, I think we should ignore this packet and keep
                        // waiting to see if we get the unsuback packet we need
                        eprintln!("Received UNSUBACK from broker with mismatched Pid.");
                    }
                },
                pkt => {
                    return Err(format!(
                        "Received unexpected packet while waiting for PINGRESP: {:?}",
                        pkt
                    )
                    .into());
                },
            }

            Ok(spec::ReturnCode::Ok)
        })
}

fn send_packet(pkt: &mqttrs::Packet, context: &mut MqttContext) -> Result<(), Box<dyn Error>> {
    // looks like you can generate a buf with dynamic size. This is
    // good because max packet size is 256MB and we don't want to make
    // something that big every time we publish anything.
    let buf_sz = std::mem::size_of::<mqttrs::Packet>()
        + match pkt {
            mqttrs::Packet::Publish(ref publish) => {
                std::mem::size_of::<mqttrs::Publish>()
                    + std::mem::size_of::<u8>() * publish.payload.len()
            }
            mqttrs::Packet::Pubrel(_) | mqttrs::Packet::Pubrec(_) => {
                std::mem::size_of::<mqttrs::Pid>()
            }
            _ => 0,
        };
    let mut buf = vec![0u8; buf_sz];
    mqttrs::encode_slice(&pkt, &mut buf).and(Ok(()))?;

    if let Err(err) = context.tcp_send(PacketTx::Mqtt(MqttPacketTx {
        pkt: buf,
        keep_alive: true,
    })) {
        match err {
            SendError(_) => {
                return Err("Cannot send publish without connection.".into());
            }
        }
    }

    Ok(())
}

fn wait_on_response_with_retry(
    pkt: &mqttrs::Packet<'_>,
    context: &mut MqttContext,
    max_retries: usize,
    retry_timeout: u64,
) -> Result<Option<(PacketRx, usize)>, Box<dyn Error>> {
    for attempt in 0..max_retries {
        match context.tcp_recv_timeout(Duration::from_secs(retry_timeout)) {
            Err(err) if err == RecvTimeoutError::Timeout => {
                println!(
                    "Timeout waiting for response. Resending previous transmission. {:?}",
                    pkt
                );
                send_packet(pkt, context)?;
            }
            Err(err) => {
                return Err(Box::new(err));
            }
            Ok(rx) => {
                return Ok(Some((rx, attempt)));
            }
        }
    }
    Ok(None) // no response received after max amount of retries
}
