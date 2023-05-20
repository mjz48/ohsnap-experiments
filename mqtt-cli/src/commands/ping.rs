use crate::cli::spec;
use crate::mqtt::{keep_alive, MqttContext};
use mqttrs::*;

/// Send a ping request to the broker. This will return an error if the
/// client is not connected to anything.
pub fn ping() -> spec::Command<MqttContext> {
    spec::Command::build("ping")
        .set_help("If connected, send a ping request to the broker.")
        .set_callback(|_command, _shell, _state, context| {
            let stream = if let Some(ref mut tcp_stream) = context.connection {
                tcp_stream as &mut dyn keep_alive::KeepAliveTcpStream
            } else {
                return Err("cannot ping broker without established connection.".into());
            };
            let tx = if let Some((_, ref tx)) = context.keep_alive {
                Some(tx.clone())
            } else {
                None
            };

            let pkt = Packet::Pingreq;
            let mut buf = [0u8; 10];

            let encoded = encode_slice(&pkt, &mut buf);
            assert!(encoded.is_ok());

            stream.write(&buf, tx).expect("Could not send request...");
            Ok(spec::ReturnCode::Ok)
        })
}
