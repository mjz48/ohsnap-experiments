use crate::cli::spec;
use crate::mqtt::MqttContext;
use crate::tcp::{self, MqttPacketTx, PacketTx};
use mqttrs::{encode_slice, Packet};
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::sync::mpsc::RecvTimeoutError;
use std::sync::mpsc::SendError;
use std::time::Duration;

const PINGRESP_TIMEOUT: u64 = 30; // in seconds

#[derive(Debug)]
pub struct PingrespTimeoutError(RecvTimeoutError);

impl Display for PingrespTimeoutError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self.0 {
            RecvTimeoutError::Timeout => {
                write!(f, "Timeout waiting for PINGRESP.")
            }
            RecvTimeoutError::Disconnected => {
                write!(f, "Disconnected while waiting for PINGRESP.")
            }
        }
    }
}
impl Error for PingrespTimeoutError {}

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

            // wait for pingresp
            let rx_pkt = context
                .tcp_recv_timeout(Duration::from_secs(PINGRESP_TIMEOUT))
                .map_err(|err| PingrespTimeoutError(err))?;

            match tcp::decode_tcp_rx(&rx_pkt)? {
                Packet::Pingresp => {
                    return Ok(spec::ReturnCode::Ok);
                }
                pkt => {
                    return Err(format!(
                        "Received unexpected packet while waiting for PINGRESP: {:?}",
                        pkt
                    )
                    .into());
                }
            }
        })
}
