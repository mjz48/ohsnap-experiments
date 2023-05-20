use crate::cli::spec;
use crate::cli::spec::flag;
use crate::mqtt::{keep_alive, MqttContext};
use mqttrs::*;
use std::io::Write;
use std::net::TcpStream;
use std::time::Duration;

use crate::cli::shell;

pub const DEFAULT_KEEP_ALIVE: u16 = 0;

/// Open a new TCP connection to a specified MQTT broker.
pub fn connect() -> spec::Command<MqttContext> {
    spec::Command::<MqttContext>::build("connect")
        .set_help("Open an MQTT connection to a broker.")
        .add_flag(
            "hostname",
            'h',
            spec::Arg::Required,
            "Hostname string or ip address of broker with optional port. E.g. -h 127.0.0.1:1883",
        )
        .add_flag(
            "port",
            'p',
            spec::Arg::Required,
            "Port num to use. Defaults to 1883 if not passed.",
        )
        .add_flag(
            "keep-alive",
            'k',
            spec::Arg::Required,
            "Specify keep alive interval (maximum number of seconds to ping broker if inactive). Default is 0.",
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

            s.write(&buf, tx)
                .expect("Could not connect to mqtt broker...");
            println!("Connected to the server!");

            context.connection = Some(stream);

            state.insert(
                shell::STATE_PROMPT_STRING.into(),
                shell::StateValue::String(context.client_id.clone()),
            );

            Ok(spec::ReturnCode::Ok)
        })
}

pub fn exit<Context: std::marker::Send>() -> spec::Command<Context> {
    spec::Command::build("exit")
        .set_help("Quit the command line interface.")
        .set_callback(|_command, _shell, _state, _context| Ok(spec::ReturnCode::Abort))
}

pub fn help<Context: std::marker::Send>() -> spec::Command<Context> {
    spec::Command::build("help")
        .set_help("Print this help message")
        .set_callback(|_command, shell, _state, _context| {
            println!("{}", shell.help());
            Ok(spec::ReturnCode::Ok)
        })
}

pub fn ping() -> spec::Command<MqttContext> {
    spec::Command::build("ping")
        .set_help("If connected, send a ping request to the broker.")
        .set_callback(|_command, _shell, _state, context| {
            let stream = if let Some(ref mut tcp_stream) = context.connection {
                tcp_stream
            } else {
                return Err("cannot ping broker without established connection.".into());
            };

            let pkt = Packet::Pingreq;
            let mut buf = [0u8; 10];

            let encoded = encode_slice(&pkt, &mut buf);
            assert!(encoded.is_ok());

            stream.write(&buf).expect("Could not send request...");

            // TODO: move this behind some TcpStream wrapper?
            if let Some((_, ref tx)) = context.keep_alive {
                tx.send(keep_alive::WakeReason::Reset).unwrap();
            }

            Ok(spec::ReturnCode::Ok)
        })
}
