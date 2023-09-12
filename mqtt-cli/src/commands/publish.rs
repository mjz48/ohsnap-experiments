use crate::cli::spec;
use crate::cli::spec::flag;
use crate::mqtt::MqttContext;
use crate::tcp::{self, MqttPacketTx, PacketRx, PacketTx};
use mqttrs::Pid;
use std::error::Error;
use std::sync::mpsc::{RecvTimeoutError, SendError};
use std::time::Duration;

const MAX_RETRIES: usize = 5;
const RETRY_TIMEOUT: u64 = 20; // in seconds

#[derive(Debug)]
enum QoSState<'pkt> {
    PublishSent(Pid, usize, mqttrs::Packet<'pkt>),
    PubrelSent(Pid, usize, mqttrs::Packet<'pkt>),
}

impl std::fmt::Display for QoSState<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            QoSState::PublishSent(pid, _, pkt) | QoSState::PubrelSent(pid, _, pkt) => {
                write!(f, "QoS::ExactlyOnce{{{:?}}} - {:?}", pid, pkt)
            }
        }
    }
}

#[derive(Debug)]
struct QoSProtocolError<'pkt>(pub QoSState<'pkt>, pub String);

impl std::fmt::Display for QoSProtocolError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Transaction failed for {}: {}", self.0, self.1)
    }
}

impl std::error::Error for QoSProtocolError<'_> {}

pub fn publish() -> spec::Command<MqttContext> {
    spec::Command::<MqttContext>::build("publish")
        .set_description("Publish a message to a specific topic path")
        .set_usage("{$name} {$flags} topic_path message")
        .add_flag(
            "dup", // TODO: need to implement flag handling
            'd',
            spec::Arg::None,
            "Set dup field. Retransmissions of packets must have this set. (For debug purposes only.)",
        )
        .add_flag(
            "qos",
            'q',
            spec::Arg::Required,
            "Specify the QoS level to be associated with this message.",
        )
        .add_flag(
            "retain", // TODO: need to implement flag handling
            'r',
            spec::Arg::None,
            "Tell the server it must retain this message.",
        )
        .set_enable(|_command, _shell, _state, context: &mut MqttContext| {
            context.tcp_write_tx.is_some()
        })
        .set_callback(|command, _shell, _state, context| {
            let qos = if let Some(flag) = command.get_flag(flag::Query::Short('q')) {
                match flag.arg().get_as::<u8>()?.unwrap() {
                    1 => mqttrs::QoS::AtLeastOnce,
                    2 => mqttrs::QoS::ExactlyOnce,
                    _ => mqttrs::QoS::AtMostOnce,
                }
            } else {
                mqttrs::QoS::AtMostOnce
            };

            let mut op_iter = command.operands().iter();

            // TODO: validate topic. must be:
            // 1. Non-empty. '/' is permitted.
            // 2. Not contain wildcard characters '#' or '*'.
            let topic = match op_iter.next() {
                Some(op) => op.value(),
                None => return Err("need to specify topic to publish.".into()),
            };

            let message = match op_iter.next() {
                Some(op) => op.value(),
                None => return Err("need to specify message to publish".into()),
            };

            drop(op_iter);

            let pid = mqttrs::Pid::new(); // TODO: implement Pid handling

            let qospid = match qos {
                mqttrs::QoS::AtMostOnce => mqttrs::QosPid::AtMostOnce,
                mqttrs::QoS::AtLeastOnce => mqttrs::QosPid::AtLeastOnce(pid),
                mqttrs::QoS::ExactlyOnce => mqttrs::QosPid::ExactlyOnce(pid),
            };

            let mut pkt = mqttrs::Packet::Publish(mqttrs::Publish {
                dup: false,                         // TODO: implement feature
                qospid,
                retain: false,                      // TODO: implement feature
                topic_name: topic,
                payload: message.as_bytes(),
            });
            send_packet(&pkt, context)?;

            // if qos > 0, need to wait for responses
            match qos {
                mqttrs::QoS::AtMostOnce => (),
                mqttrs::QoS::AtLeastOnce => {
                    println!("QoS is {:?}. Waiting for Puback from server...", qos);

                    let mut num_retries = 0;
                    if !loop {
                        match context.tcp_recv_timeout(Duration::from_secs(RETRY_TIMEOUT)) {
                            Ok(rx_pkt) => {
                                match tcp::decode_tcp_rx(&rx_pkt)? {
                                    mqttrs::Packet::Puback(resp_pid) => {
                                        if pid != resp_pid { continue; }

                                        println!("Received PubAck for {:?}", resp_pid);
                                        break true;
                                    }
                                    _ => {
                                        // ignore packets we aren't listening for
                                        continue;
                                    }
                                }
                            }
                            Err(_) => {
                                if num_retries >= MAX_RETRIES {
                                    break false;
                                }

                                println!("Timeout waiting for Puback. Resending publish command.");
                                if let mqttrs::Packet::Publish(ref mut publish) = pkt {
                                    publish.dup = true;
                                }
                                send_packet(&pkt, context)?;
                                num_retries += 1;
                            }
                        };
                    } {
                        println!("Timed out waiting for broker response for publish command. Aborting.");
                    }
                }
                mqttrs::QoS::ExactlyOnce => {
                    println!("QoS is {:?}. Waiting for Pubrec from server...", qos);
                    match handle_qos_exactly_once(
                        context,
                        QoSState::PublishSent(pid, 0, pkt),
                        MAX_RETRIES,
                        RETRY_TIMEOUT
                    ) {
                        Ok(()) => (),
                        Err(err) => {
                            println!("{}", err);
                        }

                    }
                }
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

fn handle_qos_exactly_once<'pkt>(
    context: &mut MqttContext,
    mut state: QoSState<'pkt>,
    max_retries: usize,
    retry_timeout: u64,
) -> Result<(), Box<dyn Error + 'pkt>> {
    loop {
        match state {
            QoSState::PublishSent(ref pid, ref mut retries, ref mut pkt) => {
                let rx_pkt = match wait_on_response_with_retry(
                    pkt,
                    context,
                    max_retries - *retries,
                    retry_timeout,
                )? {
                    Some((rx, attempts)) => {
                        *retries += attempts;
                        rx
                    }
                    None => {
                        return Err(Box::new(QoSProtocolError(
                            state,
                            "Timeout waiting on pubrec.".into(),
                        )));
                    }
                };

                // 1. validate that we've received pubrec
                let resp_pid = match tcp::decode_tcp_rx(&rx_pkt)? {
                    mqttrs::Packet::Pubrec(resp_pid) => resp_pid,
                    _ => {
                        // ignore packets that aren't what we're looking for
                        continue;
                    }
                };

                // 2. validate that the pid is the same
                if resp_pid != *pid {
                    // didn't receive the right pubrec packet, keep looking
                    continue;
                }

                // 3. down here we've received pubrec. Send pubrel and update state.
                let pubrel = mqttrs::Packet::Pubrel(pid.clone());
                state = QoSState::PubrelSent(*pid, 0, pubrel);
                continue;
            }
            QoSState::PubrelSent(ref pid, ref mut retries, ref mut pkt) => {
                let rx_pkt = match wait_on_response_with_retry(
                    pkt,
                    context,
                    max_retries - *retries,
                    retry_timeout,
                )? {
                    Some((rx, attempts)) => {
                        *retries += attempts;
                        rx
                    }
                    None => {
                        return Err(Box::new(QoSProtocolError(
                            state,
                            "Timeout waiting on pubcomp.".into(),
                        )));
                    }
                };

                // 1. validate that we've received pubcomp
                let resp_pid = match tcp::decode_tcp_rx(&rx_pkt)? {
                    mqttrs::Packet::Pubcomp(p) => p,
                    _ => {
                        continue;
                    } // ignore all other packets, but keep waiting
                };

                // 2. validate that the pid matches
                if resp_pid != *pid {
                    continue;
                }

                // 3. we've received pubcomp. Transaction is complete.
                return Ok(());
            }
        }
    }
}

fn wait_on_response_with_retry(
    pkt: &mut mqttrs::Packet<'_>,
    context: &mut MqttContext,
    max_retries: usize,
    retry_timeout: u64,
) -> Result<Option<(PacketRx, usize)>, Box<dyn Error>> {
    for attempt in 0..max_retries {
        match context.tcp_recv_timeout(Duration::from_secs(retry_timeout)) {
            Err(err) if err == RecvTimeoutError::Timeout => {
                println!("Timeout waiting for response. Resending previous transmission...");
                if let mqttrs::Packet::Publish(ref mut publish) = pkt {
                    publish.dup = true;
                }
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
