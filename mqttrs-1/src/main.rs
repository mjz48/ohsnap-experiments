use clap::{arg, command, value_parser, ArgAction};
use mqttrs_1::broker::{self, Broker};
use mqttrs_1::error::Error;
use std::net::{IpAddr, Ipv4Addr};

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let flags = command!()
        .arg(
            arg!(-p --port <"TCP/IP SOCKET"> "Port num for broker to listen on")
                .action(ArgAction::Set)
                .value_parser(value_parser!(u16)),
        )
        .arg(
            arg!(-i --ip <"IP ADDRESS"> "IP Address for broker to listen on")
                .action(ArgAction::Set)
                .value_parser(value_parser!(Ipv4Addr)),
        )
        .get_matches();

    let port = flags
        .get_one::<u16>("port")
        .and_then(|p| Some((*p).try_into().expect("could not case i32 to u16")))
        .unwrap_or(1883);

    let ip = IpAddr::V4(
        flags
            .get_one::<Ipv4Addr>("ip")
            .and_then(|ip| Some(*ip))
            .unwrap_or(Ipv4Addr::new(0, 0, 0, 0)),
    );

    match Broker::run(broker::Config::new(ip, port)).await {
        Ok(_) => Ok(()),
        Err(Error::TokioErr(e)) => Err(e),
        Err(err) => {
            panic!("MQTT protocol error occurred: {:?}", err);
        }
    }
}
