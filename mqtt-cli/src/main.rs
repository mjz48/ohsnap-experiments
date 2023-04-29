use bytes::BytesMut;
use mqttrs::*;
use std::fmt;
use std::io::{self, Write};
use std::net::TcpStream;

fn print_help() {
    println!("This is an MQTT command line interface for testing MQTT implementations.\n");
    println!("Usage: shell [COMMAND]\n");
    println!("Commands:");

    // probably need for loop over enum variants
    // also need to figure out how to format columns
    println!("help    Print this message.");
    println!("exit    Exit the shell.");
}

enum Command {
    Help,
    Exit,
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let command_str = match self {
            Command::Help => "Help",
            Command::Exit => "Exit",
        };

        write!(f, "{}", command_str)
    }
}

#[derive(Debug)]
struct Port {
    port: u16,
}

impl Port {
    fn new(port: u16) -> Port {
        Port {
            port,
        }
    }
}

impl fmt::Display for Port {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.port)
    }
}

#[derive(Debug)]
struct ShellContext {
    client_id: String,
    broker_hostname: String,
    broker_port: Port,
    connection: Option<TcpStream>,
}

// TODO: this seems like it should be refactored into 1) tcp connect and 2) send MQTT connect
fn connect(hostname: &str, port: Port, context: &mut ShellContext) -> std::io::Result<TcpStream> {
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
    let mut stream = TcpStream::connect(format!("{}:{}", hostname, port)).unwrap();

    stream.write(&encoded).expect("Could not connect to server...");
    println!("Connected to the server!");

    let stream_clone = stream.try_clone();
    context.connection = Some(stream);

    stream_clone
}

fn make_shell_prompt(context: &ShellContext) -> String {
    let client_id = if context.client_id.is_empty() { "mqtt" } else { &context.client_id };
    if let Some(_) = &context.connection {
        format!("{}@{}:{}>", client_id, context.broker_hostname, context.broker_port)
    } else {
        format!("{}>", client_id)
    }
}

fn parse_command(user_input: &str) -> Result<Command, String> {
    match user_input.to_lowercase().as_str() {
        "help" => { Ok(Command::Help) }
        "exit" => { Ok(Command::Exit) }
        _ => {
            return Err(format!("unknown command '{}'", user_input));
        }
    }
}

fn main() {
    print_help();

    let mut sc = ShellContext {
        client_id: "mqtt-cli".into(),
        broker_hostname: "127.0.0.1".into(),
        broker_port: Port::new(1883),
        connection: None,
    };

    connect("127.0.0.1", Port::new(1883), &mut sc).unwrap();

    println!("ShellContext: {:#?}", sc);

    loop {
        print!("{} ", make_shell_prompt(&sc));
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .expect("failed to read line");

        println!("User inputted: {}", input);
        let input = input.trim();

        match parse_command(&input) {
            Ok(command) => {
                println!("Parsed command: {}", command);
                match command {
                    Command::Help => print_help(),
                    Command::Exit => break,
                }
            }
            Err(error) => println!("Error: {:#?}", error),
        }
    }
}
