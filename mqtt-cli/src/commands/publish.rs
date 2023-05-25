use crate::cli::spec;
use crate::mqtt::MqttContext;
use crate::tcp::{MqttPacketTx, PacketTx};
use std::sync::mpsc::SendError;

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
            "qos", // TODO: need to implement flag handling
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
        .set_callback(|command, _shell, _state, context| {
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

            let pkt = mqttrs::Packet::Publish(mqttrs::Publish {
                dup: false,                         // TODO: implement feature
                qospid: mqttrs::QosPid::AtMostOnce, // TODO: implement feature
                retain: false,                      // TODO: implement feature
                topic_name: topic,
                payload: message.as_bytes(),
            });

            // looks like you can generate a buf with dynamic size. This is
            // good because max packet size is 256MB and we don't want to make
            // something that big every time we publish anything.
            let buf_sz = if let mqttrs::Packet::Publish(ref publish) = pkt {
                std::mem::size_of::<mqttrs::Publish>()
                    + std::mem::size_of::<u8>() * publish.payload.len()
            } else {
                0
            };
            let mut buf = vec![0u8; buf_sz];
            mqttrs::encode_slice(&pkt, &mut buf)?;

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

            Ok(spec::ReturnCode::Ok)
        })
}
