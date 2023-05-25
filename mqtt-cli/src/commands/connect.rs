use std::fmt::{self, Display, Formatter};
use std::error::Error;
use crate::cli::shell;
use crate::cli::spec;
use crate::cli::spec::flag;
use crate::mqtt::{keep_alive, MqttContext};
use crate::tcp::{self, PacketRx, PacketTx, MqttPacketTx};
use std::sync::mpsc::{self, RecvTimeoutError};
use colored::Colorize;
use mqttrs::{self, Connect, Packet};
use std::time::Duration;

pub const DEFAULT_KEEP_ALIVE: u16 = 0;
pub const CONNACK_TIMEOUT: u64 = 30; // seconds

#[derive(Debug)]
pub struct ConnackTimeoutError(RecvTimeoutError);

impl Display for ConnackTimeoutError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self.0 {
            RecvTimeoutError::Timeout => {
                write!(f, "Timeout waiting for CONNACK.")
            },
            RecvTimeoutError::Disconnected => {
                write!(f, "Disconnected while waiting for CONNACK.")
            }
        }
    }
}
impl Error for ConnackTimeoutError {}

/// Open a new TCP connection to a specified MQTT broker.
pub fn connect() -> spec::Command<MqttContext> {
    spec::Command::<MqttContext>::build("connect")
        .set_help("Open an MQTT connection to a broker.")
        .add_flag(
            "clean-session", // TODO: need to implement flag handling
            'c',
            spec::Arg::None,
            "Specify if this is a clean session. If not present, the broker must resume previous session if any. If set, the broker will ignore any previous session if any.",
        )
        .add_flag(
            "hostname",
            'h',
            spec::Arg::Required,
            "Hostname string or ip address of broker with optional port. E.g. -h 127.0.0.1:1883",
        )
        .add_flag(
            "keep-alive", // TODO: need to implement flag handling
            'k',
            spec::Arg::Required,
            "Specify keep alive interval (maximum number of seconds to ping broker if inactive). Default is 0.",
        )
        .add_flag(
            "password", // TODO: need to implement flag handling
            's',
            spec::Arg::Required,
            "Password for broker to authenticate. Username must also be present if this flag is set.",
        )
        .add_flag(
            "port",
            'p',
            spec::Arg::Required,
            "Port num to use. Defaults to 1883 if not passed.",
        )
        .add_flag(
            "qos", // TODO: need to implement flag handling
            'q',
            spec::Arg::Required,
            "Set the QoS level of the Will Message. This will be ignored if the will flag is not present.",
        )
        .add_flag(
            "retain", // TODO: need to implement flag handling
            'r',
            spec::Arg::None,
            "Set will_retain flag to 1. If will_retain is set, the broker will retain published messages.",
        )
        .add_flag(
            "username",
            'u',
            spec::Arg::Required,
            "Enter a username for the broker to authenticate."
        )
        .add_flag(
            "will", // TODO: need to implement flag handling
            'w',
            spec::Arg::None,
            "Set will_flag if present. If will_flag is set, this indicates the broker must store a Will Message.",
        )
        .set_callback(|command, _shell, state, context: &mut MqttContext| {
            let keep_alive = if let Some(flag) = command.get_flag(flag::Query::Short('k')) {
                flag.arg().get_as::<u16>()?.unwrap()
            } else {
                DEFAULT_KEEP_ALIVE
            };

            let hostname = if let Some(flag) = command.get_flag(flag::Query::Short('h')) {
                flag.arg().get_as::<String>()?.unwrap()
            } else {
                context.broker.hostname.to_owned()
            };

            let port = if let Some(flag) = command.get_flag(flag::Query::Short('p')) {
                flag.arg().get_as::<u16>()?.unwrap()
            } else {
                context.broker.port.to_owned()
            };

            context.username = command.get_flag(
                flag::Query::Short('u'))
                    .and_then(|f| {
                        match f.arg().get_as::<String>() {
                            Ok(a) => a,
                            Err(_) => None,
                        }
                    }
                );

            // encode Connect packet
            let pkt = Packet::Connect(Connect {
                protocol: mqttrs::Protocol::MQTT311,
                keep_alive,
                client_id: &context.client_id,
                clean_session: true,
                last_will: None,
                username: if let Some(ref u) = context.username { Some(u) } else { None },
                password: None,
            });
            let mut buf = vec![0u8; std::mem::size_of::<Connect>()];
            mqttrs::encode_slice(&pkt, &mut buf)?;

            // initialize keep alive thread
            if keep_alive > 0 {
                let keep_alive_context = keep_alive::spawn_keep_alive_thread(
                    Duration::from_secs(keep_alive.into()),
                    state
                )?;

                // suspend keep_alive until the connection is established
                let _ = &keep_alive_context.keep_alive_tx.send(keep_alive::Msg::Suspend)?;

                // set context variable
                context.keep_alive_tx = Some(keep_alive_context.keep_alive_tx);
            }
            
            // set up tcp connection and associated channels
            let (tcp_read_tx, tcp_read_rx) = mpsc::channel::<PacketRx>();
            let tcp_context = tcp::spawn_tcp_thread(
                &hostname, port, context.keep_alive_tx.clone(), tcp_read_tx)?;

            context.tcp_write_tx = Some(tcp_context.tcp_write_tx.clone());
            context.tcp_read_rx = Some(tcp_read_rx);

            // send connect packet
            context.tcp_send(PacketTx::Mqtt(MqttPacketTx {
                pkt: buf,
                keep_alive: true,
            }))?;

            // wait for connack packet
            let rx_pkt = context
                .tcp_recv_timeout(Duration::from_secs(CONNACK_TIMEOUT))
                .map_err(|err| { ConnackTimeoutError(err) })?;

            match tcp::decode_tcp_rx(&rx_pkt)? {
                Packet::Connack(connack) => {
                    println!("Received CONNACK: {:?}", connack);
                }
                pkt => {
                    return Err(format!("Received unexpected packet while waiting for CONNACK: {:?}", pkt).into());
                }
            }
            
            println!("Connected to the server!");

            // unsuspend keep alive thread
            if let Some(ref keep_alive_tx) = context.keep_alive_tx {
                keep_alive_tx.send(keep_alive::Msg::Resume)?;
            }

            let prompt = if let Some(ref username) = context.username {
                format!("{}@{}:{}", username, hostname, port)
            } else {
                format!("{}@{}:{}", context.client_id, hostname, port)
            }.bright_yellow();

            state.insert(
                shell::STATE_PROMPT_STRING.into(),
                shell::StateValue::RichString(prompt),
            );

            Ok(spec::ReturnCode::Ok)
        })
}
