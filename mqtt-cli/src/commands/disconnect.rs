use crate::cli::shell;
use crate::cli::spec;
use crate::mqtt::{keep_alive, MqttContext};

pub fn disconnect() -> spec::Command<MqttContext> {
    spec::Command::<MqttContext>::build("disconnect")
        .set_help("Disconnect from the a broker.")
        .set_callback(|_command, _shell, state, context| {
            let stream = if let Some(ref mut tcp_stream) = context.connection {
                tcp_stream as &mut dyn keep_alive::KeepAliveTcpStream
            } else {
                return Err("No connection is present.".into());
            };

            let keep_alive_tx = if let Some((_, ref tx)) = context.keep_alive {
                Some(tx.clone())
            } else {
                None
            };

            // send disconnect packet to broker
            let pkt = mqttrs::Packet::Disconnect;
            let mut buf = vec![0u8, 0];

            mqttrs::encode_slice(&pkt, &mut buf)?;
            stream
                .write(&buf, keep_alive_tx)
                .expect("Could not send disconnect message.");

            let broker_name = format!("{}:{}", context.broker.hostname, context.broker.port);
            println!("Disconnected from {}", broker_name);

            // kill keep alive thread
            context.keep_alive = None;

            // reset prompt to default
            state.insert(
                shell::STATE_PROMPT_STRING.into(),
                shell::StateValue::String(context.prompt_string.to_owned()),
            );

            Ok(spec::ReturnCode::Ok)
        })
}
