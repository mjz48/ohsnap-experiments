use crate::cli::shell;
use crate::cli::spec;
use crate::cli::spec::flag;
use crate::commands::util;
use crate::mqtt::{keep_alive, MqttContext};
use crate::tcp::{self, MqttPacketTx, PacketRx, PacketTx};
use mqttrs::{self, Connect, Packet};
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

const DEFAULT_KEEP_ALIVE: u16 = 0;
const CONNACK_TIMEOUT: u64 = 30; // seconds

#[derive(Debug)]
pub struct ConnackTimeoutError(RecvTimeoutError);

impl Display for ConnackTimeoutError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self.0 {
            RecvTimeoutError::Timeout => {
                write!(f, "Timeout waiting for CONNACK.")
            }
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
        .set_description("Open an MQTT connection to a broker")
        .set_usage("{$name} {$flags}")
        .add_flag(
            "clean-session", // TODO: need to implement flag handling
            'c',
            spec::Arg::None,
            "Specify to the broker to use a clean session.",
        )
        .add_flag(
            "hostname",
            'h',
            spec::Arg::Required,
            "Hostname string or ip address of broker with optional port. E.g. -h 127.0.0.1:1883",
        )
        .add_flag(
            "keep-alive",
            'k',
            spec::Arg::Required,
            "Specify keep alive interval (max seconds to ping broker if inactive). Default 0.",
        )
        .add_flag(
            "password", // TODO: need to implement flag handling
            's',
            spec::Arg::Required,
            "Password for broker to authenticate. Username required if this flag is set.",
        )
        .add_flag(
            "port",
            'p',
            spec::Arg::Required,
            "Port num to use. Defaults to 1883 if not passed.",
        )
        .add_flag(
            "qos",
            'q',
            spec::Arg::Required,
            "Set the QoS level of the Will Message. Will flag must be set if this is set.",
        )
        .add_flag(
            "retain",
            'r',
            spec::Arg::None,
            "Specify to the broker to retain published messages from this client.",
        )
        .add_flag(
            "topic",
            't',
            spec::Arg::Required,
            "Specify will topic. Message flag must be set if this is set.",
        )
        .add_flag(
            "username",
            'u',
            spec::Arg::Required,
            "Enter a username for the broker to authenticate. (Also known as 'client id'.)",
        )
        .add_flag(
            "will",
            'w',
            spec::Arg::Required,
            "Specify will message contents. Topic flag must be set if this is set.",
        )
        .set_enable(|_command, _shell, _state, context: &mut MqttContext| {
            !context.tcp_write_tx.is_some()
        })
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

            let will_qos = if let Some(flag) = command.get_flag(flag::Query::Short('q')) {
                flag.arg().get_as::<u8>()?.unwrap()
            } else {
                0
            };

            let will_retain = command.get_flag(flag::Query::Short('r')).is_some();

            let will_topic = if let Some(flag) = command.get_flag(flag::Query::Short('t')) {
                flag.arg().get_as::<String>()?.unwrap()
            } else {
                "".into()
            };

            let will = if let Some(flag) = command.get_flag(flag::Query::Short('w')) {
                flag.arg().get_as::<String>()?
            } else {
                None
            };

            if let Some(client_id) = command
                .get_flag(flag::Query::Short('u'))
                .and_then(|f| match f.arg().get_as::<String>() {
                    Ok(a) => a,
                    Err(_) => None,
                })
            {
                context.client_id = client_id;
            }

            let last_will = if let Some(ref will_message) = will {
                Some(mqttrs::LastWill {
                    topic: will_topic.as_str(),
                    message: will_message.as_bytes(),
                    qos: match will_qos {
                        0 => mqttrs::QoS::AtMostOnce,
                        1 => mqttrs::QoS::AtLeastOnce,
                        2 => mqttrs::QoS::ExactlyOnce,
                        _ => {
                            return Err(Box::new(io::Error::new(
                                io::ErrorKind::InvalidInput,
                                format!(
                                    "Invalid will_qos specified: {}. Expected 0, 1 or 2.",
                                    will_qos
                                ),
                            )))
                        }
                    },
                    retain: will_retain,
                })
            } else {
                None
            };

            // encode Connect packet
            let pkt = Packet::Connect(Connect {
                protocol: mqttrs::Protocol::MQTT311,
                keep_alive,
                client_id: &context.client_id,
                clean_session: true,
                last_will,
                username: Some(&context.client_id),
                password: None,
            });
            let mut buf = vec![0u8; std::mem::size_of::<Connect>()];
            mqttrs::encode_slice(&pkt, &mut buf)?;

            // initialize keep alive thread
            if keep_alive > 0 {
                let keep_alive_context = keep_alive::spawn_keep_alive_thread(
                    Duration::from_secs(keep_alive.into()),
                    state,
                )?;

                // suspend keep_alive until the connection is established
                let _ = &keep_alive_context
                    .keep_alive_tx
                    .send(keep_alive::Msg::Suspend)?;

                // set context variable
                context.keep_alive_tx = Some(keep_alive_context.keep_alive_tx);
            }

            // set up tcp connection and associated channels
            let state_cmd_tx = match state.get(shell::STATE_CMD_TX.into()) {
                Some(shell::StateValue::Sender(tx)) => tx.clone(),
                Some(_) | None => {
                    return Err(Box::new(io::Error::new(
                        io::ErrorKind::NotFound,
                        "command queue tx not found in shell state",
                    )));
                }
            };

            let (tcp_read_tx, tcp_read_rx) = mpsc::channel::<PacketRx>();
            let tcp_context = tcp::spawn_tcp_thread(
                &hostname,
                port,
                context.keep_alive_tx.clone(),
                tcp_read_tx,
                state_cmd_tx.clone(),
            )?;

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
                .map_err(|err| ConnackTimeoutError(err))?;

            match tcp::decode_tcp_rx(&rx_pkt)? {
                Packet::Connack(connack) => {
                    println!("Received CONNACK: {:?}", connack);
                }
                pkt => {
                    return Err(format!(
                        "Received unexpected packet while waiting for CONNACK: {:?}",
                        pkt
                    )
                    .into());
                }
            }

            println!("Connected to the server!");

            // unsuspend keep alive thread
            if let Some(ref keep_alive_tx) = context.keep_alive_tx {
                keep_alive_tx.send(keep_alive::Msg::Resume)?;
            }

            state.insert(
                shell::STATE_PROMPT_STRING.into(),
                shell::StateValue::RichString(util::generate_prompt(&context)),
            );

            Ok(spec::ReturnCode::Ok)
        })
}
