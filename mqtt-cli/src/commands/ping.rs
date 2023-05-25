use crate::cli::spec;
use crate::mqtt::MqttContext;
use crate::tcp::{MqttPacketTx, PacketTx};
use mqttrs::{encode_slice, Packet};
use std::sync::mpsc::SendError;

/// Send a ping request to the broker. This will return an error if the
/// client is not connected to anything.
pub fn ping() -> spec::Command<MqttContext> {
    spec::Command::build("ping")
        .set_help("If connected, send a ping request to the broker.")
        .set_callback(|_command, _shell, _state, context| {
            let pkt = Packet::Pingreq;
            let mut buf = vec![0u8; 10];
            encode_slice(&pkt, &mut buf)?;

            if let Err(err) = context.tcp_send(PacketTx::Mqtt(MqttPacketTx {
                pkt: buf,
                keep_alive: true,
            })) {
                match err {
                    SendError(_) => {
                        return Err("No tcp connection to send ping.".into());
                    }
                }
            };

            Ok(spec::ReturnCode::Ok)
        })
}
