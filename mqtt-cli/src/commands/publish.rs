use crate::cli::spec;
use crate::cli::spec::flag;
use crate::mqtt::MqttContext;
use crate::tcp::{self, MqttPacketTx, PacketTx};
use std::error::Error;
use std::sync::mpsc::SendError;
use std::time::Duration;

const MAX_RETRIES: usize = 5;
const RETRY_TIMEOUT: u64 = 20; // in seconds

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
                mqttrs::QoS::AtLeastOnce
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
                                if num_retries == MAX_RETRIES {
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
                        println!("Max retries reached for publish command. Aborting.");
                    }
                }
                mqttrs::QoS::ExactlyOnce => {
                    // TODO: implement
                }
            }

            Ok(spec::ReturnCode::Ok)
        })
}

fn send_packet(pkt: &mqttrs::Packet, context: &mut MqttContext) -> Result<(), Box<dyn Error>> {
    // looks like you can generate a buf with dynamic size. This is
    // good because max packet size is 256MB and we don't want to make
    // something that big every time we publish anything.
    let buf_sz = if let mqttrs::Packet::Publish(ref publish) = pkt {
        std::mem::size_of::<mqttrs::Publish>() + std::mem::size_of::<u8>() * publish.payload.len()
    } else {
        0
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
