use crate::cli::spec;
use crate::mqtt::MqttContext;
use crate::tcp;
use crate::tcp::{MqttPacketTx, PacketTx};
use mqttrs::Packet;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::sync::mpsc::{RecvTimeoutError, SendError};
use std::time::Duration;

const UNSUBACK_TIMEOUT: u64 = 30; // seconds

#[derive(Debug)]
pub struct UnsubackTimeoutError(RecvTimeoutError);

impl Display for UnsubackTimeoutError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self.0 {
            RecvTimeoutError::Timeout => {
                write!(f, "Timeout waiting for UNSUBACK.")
            }
            RecvTimeoutError::Disconnected => {
                write!(f, "Disconnected while waiting for UNSUBACK.")
            }
        }
    }
}

impl Error for UnsubackTimeoutError {}

pub fn unsubscribe() -> spec::Command<MqttContext> {
    spec::Command::<MqttContext>::build("unsubscribe")
        .set_description("Unsubscribe from one or more topics")
        .set_usage("{$name} topic1 [topic2 ...]")
        .set_callback(|command, _shell, _state, context| {
            if command.operands().len() == 0 {
                return Err("Must provide at least one topic path to unsubscribe from.".into());
            }

            let mut topics = vec![];
            let mut topics_size = 0;
            for op in command.operands().iter() {
                topics.push(op.value().to_owned());
                topics_size += op.value().len();
            }

            // TODO: implement pid handling
            let pkt = Packet::Unsubscribe(mqttrs::Unsubscribe {
                pid: mqttrs::Pid::new(),
                topics,
            });

            let buf_sz = std::mem::size_of::<mqttrs::Unsubscribe>() + topics_size;
            let mut buf = vec![0u8; buf_sz];
            mqttrs::encode_slice(&pkt, &mut buf)?;

            if let Err(SendError(_)) = context.tcp_send(PacketTx::Mqtt(MqttPacketTx {
                pkt: buf,
                keep_alive: true,
            })) {
                return Err("Cannot unsubscribe without connection to broker.".into());
            }

            // wait for unsuback packet from broker
            let rx_pkt = context
                .tcp_recv_timeout(Duration::from_secs(UNSUBACK_TIMEOUT))
                .map_err(|err| UnsubackTimeoutError(err))?;

            match tcp::decode_tcp_rx(&rx_pkt)? {
                Packet::Unsuback(_pid) => {
                    () // don't need to do anything other than receive this right now
                }
                pkt => {
                    return Err(format!(
                        "Received unexpected packet while waiting for UNSUBACK: {:?}",
                        pkt
                    )
                    .into());
                }
            }

            Ok(spec::ReturnCode::Ok)
        })
}
