use crate::cli::shell;
use crate::cli::spec;
use crate::cli::spec::flag;
use crate::mqtt::{keep_alive, MqttContext};
use crate::tcp::{MqttPacketTx, PacketTx};

pub fn disconnect() -> spec::Command<MqttContext> {
    spec::Command::<MqttContext>::build("disconnect")
        .set_description("Disconnect from the broker")
        .set_usage("{$name} {$flags}")
        .add_flag(
            "cleanup",
            'c',
            spec::Arg::None,
            "Run all disconnect actions except closing tcp connection. (For internal usage.)",
        )
        .set_callback(|command, _shell, state, context| {
            let cleanup = command.get_flag(flag::Query::Short('c')).is_some();

            if !cleanup {
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
            }

            if cleanup {
                println!(
                    "Broker {}:{} has disconnected.",
                    context.broker.hostname, context.broker.port
                );
            } else {
                println!(
                    "Disconnected from {}:{}",
                    context.broker.hostname, context.broker.port
                );
            }

            // kill keep alive thread
            if let Some(ref tx) = context.keep_alive_tx {
                if let Err(err) = tx.send(keep_alive::Msg::Kill) {
                    if !cleanup {
                        return Err(Box::new(err));
                    }
                }
            }
            context.keep_alive_tx = None;

            // kill the tcp thread
            if let Some(ref tx) = context.tcp_write_tx {
                if let Err(err) = tx.send(PacketTx::Close) {
                    if !cleanup {
                        return Err(Box::new(err));
                    }
                }
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
