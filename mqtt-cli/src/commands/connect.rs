use crate::cli::shell;
use crate::cli::spec;
use crate::cli::spec::flag;
use crate::mqtt::{keep_alive, MqttContext};
use crate::tcp_stream;
use mqttrs::*;
use std::net::TcpStream;
use std::time::Duration;

pub const DEFAULT_KEEP_ALIVE: u16 = 0;
pub const CONNACK_TIMEOUT: u16 = 2; // 30; // seconds

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
            "username", // TODO: need to implement flag handling
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
            let kp = if let Some(flag) = command.get_flag(flag::Query::Short('k')) {
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

            if kp > 0 {
                keep_alive::keep_alive(Duration::from_secs(kp.into()), state, context);
            }

            let mut buf = [0u8; 1024];
            let pkt = Packet::Connect(Connect {
                protocol: Protocol::MQTT311,
                keep_alive: kp,
                client_id: &context.client_id,
                clean_session: true,
                last_will: None,
                username: None,
                password: None,
            });

            let mut stream = TcpStream::connect(format!( "{}:{}", hostname, port))?;
            let s = &mut stream as &mut dyn keep_alive::KeepAliveTcpStream;
            let tx = if let Some((_, ref tx)) = context.keep_alive {
                Some(tx.clone())
            } else {
                None
            };

            let encoded = encode_slice(&pkt, &mut buf);
            assert!(encoded.is_ok());

            // send the connect packet to broker
            s.write(&buf, tx)
                .expect("Could not connect to mqtt broker...");

            // need to receive a connack packet from broker
            let old_stream_timeout = stream.read_timeout()?;
            stream.set_read_timeout(Some(Duration::from_secs(CONNACK_TIMEOUT.into())))?;

            let mut ret_pkt_buf: Vec<u8> = Vec::new();
            match tcp_stream::read_and_decode(&mut stream, &mut ret_pkt_buf) {
                Ok(Packet::Connack(connack)) => {
                    println!("Received connack! {:?}", connack);
                }
                Ok(pkt) => {
                    return Err(format!("Received unexpected packet during connection attempt:\n{:?}", pkt).into());
                }
                Err(err) => {
                    return Err(format!("Timeout waiting for CONNACK packet:\n{:?}", err).into());
                }
            }
            stream.set_read_timeout(old_stream_timeout)?;
            println!("Connected to the server!");

            context.connection = Some(stream);

            state.insert(
                shell::STATE_PROMPT_STRING.into(),
                shell::StateValue::String(context.client_id.clone()),
            );

            Ok(spec::ReturnCode::Ok)
        })
}
