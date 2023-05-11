use bytes::BytesMut;
use crate::cli::spec;
use crate::mqtt::{keep_alive, MqttContext};
use mqttrs::*;
use std::io::Write;
use std::net::TcpStream;
use std::time::Duration;

use crate::cli::shell;

/// Open a new TCP connection to a specified MQTT broker.
pub fn connect() -> spec::Command<MqttContext> {
    spec::Command::<MqttContext>::build("connect")
        .set_help("Open an MQTT connection to a broker.")
        .add_flag("hostname", 'h', spec::Arg::Required, "Hostname string or ip address of broker with optional port. E.g. -h 127.0.0.1:1883")
        .add_flag("port", 'p', spec::Arg::Required, "Port num to use. Defaults to 1883 if not passed.")
        .set_callback(| _command, _shell, state, context: &mut MqttContext | {
            let mut buf = [0u8; 1024];
            let pkt = Packet::Connect(Connect {
                protocol: Protocol::MQTT311,
                keep_alive: 30,
                client_id: &context.client_id,
                clean_session: true,
                last_will: None,
                username: None,
                password: None,
            });

            let encoded = encode_slice(&pkt, &mut buf);
            assert!(encoded.is_ok());

            let buf = BytesMut::from(&buf[..encoded.unwrap()]);
            assert_eq!(&buf[14..], context.client_id.as_bytes());

            let encoded = buf.clone();
            let mut stream = TcpStream::connect(
                format!("{}:{}", context.broker.hostname, context.broker.port))?;
            
            stream.write(&encoded).expect("Could not connect to mqtt broker...");
            println!("Connected to the server!");
            
            context.connection = Some(stream);

            state.insert(shell::STATE_PROMPT_STRING.into(), context.client_id.clone());

            // DELETEME
            //keep_alive::keep_alive(Duration::from_secs(4), context);

            Ok(spec::ReturnCode::Ok)
        })
}

pub fn exit<Context>() -> spec::Command<Context> {
    spec::Command::build("exit")
        .set_help("Quit the command line interface.")
        .set_callback(| _command, _shell, _state, _context | {
            Ok(spec::ReturnCode::Abort)
        })
}

pub fn help<Context>() -> spec::Command<Context> {
    spec::Command::build("help")
        .set_help("Print this help message")
        .set_callback(| _command, shell, _state,  _context | {
            println!("{}", shell.help());
            Ok(spec::ReturnCode::Ok)
        })
}

pub fn ping() -> spec::Command<MqttContext> {
    spec::Command::build("ping")
        .set_help("If connected, send a ping request to the broker.")
        .set_callback(| _command, _shell, _state, context | {
            let stream = if let Some(ref mut tcp_stream) = context.connection {
                tcp_stream
            } else {
                return Err("cannot ping broker without established connection.".into());
            };

            let pkt = Packet::Pingreq;
            let mut buf = [0u8; 1024];

            let encoded = encode_slice(&pkt, &mut buf);
            assert!(encoded.is_ok());

            stream.write(&buf).expect("Could not send request...");
            
            // TODO: reset keep_alive ping

            Ok(spec::ReturnCode::Ok)
        })
}
