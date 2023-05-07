use bytes::BytesMut;
use crate::cli::spec;
use crate::mqtt::MqttContext;
use mqttrs::*;
use std::io::Write;
use std::net::TcpStream;

/// Open a new TCP connection to a specified MQTT broker.
pub fn connect() -> spec::Command<MqttContext> {
    spec::Command::<MqttContext>::build("connect")
        .set_help("Open an MQTT connection to a broker.")
        .add_flag("hostname", 'h', spec::Arg::Required, "Hostname string or ip address of broker with optional port. E.g. -h 127.0.0.1:1883")
        .add_flag("port", 'p', spec::Arg::Required, "Port num to use. Defaults to 1883 if not passed.")
        .set_callback(| _command, _shell, context: &mut MqttContext | {
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

            // TODO: need to change shell to mutable reference and this breaks a lot of stuff
            //shell.set_state(shell::STATE_PROMPT_STRING, &context.client_id);

            Ok(spec::ReturnCode::Ok)
        })
}

pub fn exit<Context>() -> spec::Command<Context> {
    spec::Command::build("exit")
        .set_help("Quit the command line interface.")
        .set_callback(| _command, _shell, _context | {
            Ok(spec::ReturnCode::Abort)
        })
}

pub fn help<Context>() -> spec::Command<Context> {
    spec::Command::build("help")
        .set_help("Print this help message")
        .set_callback(| _command, shell, _context | {
            println!("{}", shell.help());
            Ok(spec::ReturnCode::Ok)
        })
}
