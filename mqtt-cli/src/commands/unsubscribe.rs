use crate::cli::spec;
use crate::mqtt::keep_alive::KeepAliveTcpStream;
use crate::mqtt::MqttContext;
use crate::tcp_stream;
use mqttrs::Packet;
use std::time::Duration;

const UNSUBACK_TIMEOUT: u16 = 30; // seconds

pub fn unsubscribe() -> spec::Command<MqttContext> {
    spec::Command::<MqttContext>::build("unsubscribe")
        .set_help("Unsubscribe from one or more topics.")
        .set_callback(|command, _shell, _state, context| {
            if command.operands().len() == 0 {
                return Err("Must provide at least one topic path to unsubscribe from.".into());
            }

            let mut topics = vec![];
            for op in command.operands().iter() {
                topics.push(op.value().to_owned());
            }

            let stream = if let Some(ref mut tcp_stream) = context.connection {
                tcp_stream
            } else {
                return Err("Connection to broker is not present.".into());
            };

            let keep_alive_tx = if let Some((_, ref tx)) = context.keep_alive {
                Some(tx.clone())
            } else {
                None
            };

            // TODO: implement pid handling
            let pkt = Packet::Unsubscribe(mqttrs::Unsubscribe {
                pid: mqttrs::Pid::new(),
                topics,
            });

            let buf_sz = std::mem::size_of::<mqttrs::Unsubscribe>();
            let mut buf = vec![0u8; buf_sz];

            mqttrs::encode_slice(&pkt, &mut buf)?;
            (stream as &mut dyn KeepAliveTcpStream)
                .write(&buf, keep_alive_tx)
                .expect("Could not send unsubscribe message.");

            // wait for unsuback packet from broker
            let old_stream_timeout = stream.read_timeout()?;
            stream.set_read_timeout(Some(Duration::from_secs(UNSUBACK_TIMEOUT.into())))?;

            match tcp_stream::read_and_decode(stream, &mut Vec::new()) {
                Ok(Packet::Unsuback(_pid)) => {
                    () // don't need to do anything other than receive this right now
                }
                Ok(pkt) => {
                    return Err(format!(
                        "Received unexpected packet during unsubscribe attempt:\n{:?}",
                        pkt
                    )
                    .into());
                }
                Err(err) => {
                    return Err(format!("Timeout waiting for UNSUBACK packet:\n{:?}", err).into());
                }
            }
            stream.set_read_timeout(old_stream_timeout)?;

            Ok(spec::ReturnCode::Ok)
        })
}
