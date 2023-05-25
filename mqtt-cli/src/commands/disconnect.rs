use crate::cli::shell;
use crate::cli::spec;
use crate::mqtt::{keep_alive, MqttContext};
use crate::tcp::{MqttPacketTx, PacketTx};

pub fn disconnect() -> spec::Command<MqttContext> {
    spec::Command::<MqttContext>::build("disconnect")
        .set_help("Disconnect from the a broker.")
        .set_callback(|_command, _shell, state, context| {
            if context.tcp_write_tx.is_none() {
                println!("Not connected to broker.");
                return Ok(spec::ReturnCode::Ok);
            }

            let pkt = mqttrs::Packet::Disconnect;
            let mut buf = vec![0u8, 0];
            mqttrs::encode_slice(&pkt, &mut buf)?;

            context.tcp_send(PacketTx::Mqtt(MqttPacketTx {
                pkt: buf,
                keep_alive: false,
            }))?;

            println!(
                "Disconnected from {}",
                format!("{}:{}", context.broker.hostname, context.broker.port)
            );

            // kill keep alive thread
            if let Some(ref tx) = context.keep_alive_tx {
                tx.send(keep_alive::Msg::Kill)?;
            }
            context.keep_alive_tx = None;

            // kill the tcp thread
            if let Some(ref tx) = context.tcp_write_tx {
                tx.send(PacketTx::Close)?;
            }
            context.tcp_write_tx = None;
            context.tcp_read_rx = None;

            // reset prompt to default
            state.insert(
                shell::STATE_PROMPT_STRING.into(),
                shell::StateValue::String(context.prompt_string.to_owned()),
            );

            Ok(spec::ReturnCode::Ok)
        })
}
