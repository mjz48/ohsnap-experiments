use crate::cli::spec;
use crate::mqtt::{keep_alive, MqttContext};

pub fn publish() -> spec::Command<MqttContext> {
    spec::Command::<MqttContext>::build("publish")
        .set_help("Publish a message. Usage: publish [topic] [message]")
        .add_flag(
            "dup", // TODO: need to implement flag handling
            'd',
            spec::Arg::None,
            "Specify if the dup field is set. Dup is set if this packet is a retransmission. This flag should only be used for debug purposes.",
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
            "Tell the server that is must retain this message. Without this flag, the server will not retain messages.",
        )
        .set_callback(|command, _shell, _state, context| {
            let stream = if let Some(ref mut tcp_stream) = context.connection {
                tcp_stream as &mut dyn keep_alive::KeepAliveTcpStream
            } else {
                return Err("cannot publish without established connection.".into());
            };

            let keep_alive_tx = if let Some((_, ref tx)) = context.keep_alive {
                Some(tx.clone())
            } else {
                None
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

            let encoded = mqttrs::encode_slice(&pkt, &mut buf);
            assert!(encoded.is_ok());

            stream
                .write(&buf, keep_alive_tx)
                .expect("Could not publish message.");

            Ok(spec::ReturnCode::Ok)
        })
}
