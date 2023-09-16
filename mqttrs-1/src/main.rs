use clap::{arg, command, value_parser, ArgAction};
use mqttrs_1::{
    broker::{Broker, Config},
    error::Error,
};
use simplelog::{
    ColorChoice, CombinedLogger, Config as SimpleLogConfig, LevelFilter, TermLogger, TerminalMode,
    WriteLogger,
};
use std::{
    fs::{self, OpenOptions},
    net::{IpAddr, Ipv4Addr},
};
use tokio::io;

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let flags = command!()
        .arg(
            arg!(-d --log_dir <"DIRECTORY"> "Directory to write log files. Will only be used if verbosity is not 'off'.")
                .action(ArgAction::Set)
        )
        .arg(
            arg!(-f --log_file <"FILENAME"> "Filename to write log. Use with --log_dir flag. Will only be used if verbosity is not 'off'.")
                .action(ArgAction::Set)
        )
        .arg(
            arg!(-i --ip <"IP ADDRESS"> "IP Address for broker to listen on")
                .action(ArgAction::Set)
                .value_parser(value_parser!(Ipv4Addr)),
        )
        .arg(
            arg!(-m --max_retries <"MAX RETRIES"> "Maximum number of packet retransmission attempts before aborting. Will default to infinite retries.")
                .action(ArgAction::Set)
                .value_parser(value_parser!(u16)),
        )
        .arg(
            arg!(-p --port <"TCP/IP SOCKET"> "Port num for broker to listen on")
                .action(ArgAction::Set)
                .value_parser(value_parser!(u16)),
        )
        .arg(
            arg!(-r --retry <"DURATION"> "Time to wait before re-sending QoS>0 packets (in seconds).")
                .action(ArgAction::Set)
        )
        .arg(
            arg!(-t --timeout <"DURATION"> "Default timeout interval. E.g. for connections, etc. (in seconds). Separate from QoS retry interval.")
                .action(ArgAction::Set)
        )
        .arg(
            arg!(-v --verbosity <"VERBOSITY"> "Specify log level verbosity (values=off|error|warn|info|debug|trace)")
                .action(ArgAction::Set)
        )
        .get_matches();

    let log_dir = flags
        .get_one::<String>("log_dir")
        .and_then(|val| Some(val.clone()))
        .unwrap_or("log".to_string());

    let log_filename = flags
        .get_one::<String>("log_file")
        .and_then(|val| Some(val.clone()))
        .unwrap_or("broker.log".to_string());

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

    let log_level = flags
        .get_one::<String>("verbosity")
        .and_then(|v| match &v.to_lowercase()[..] {
            "off" => Some(LevelFilter::Off),
            "error" => Some(LevelFilter::Error),
            "warn" => Some(LevelFilter::Warn),
            "info" => Some(LevelFilter::Info),
            "debug" => Some(LevelFilter::Debug),
            "trace" => Some(LevelFilter::Trace),
            _ => None,
        })
        .unwrap_or(LevelFilter::Error);

    let max_retries = flags.get_one::<u16>("max_retries").unwrap_or(&0).to_owned();

    let retry_interval = flags.get_one::<u32>("retry").unwrap_or(&20).to_owned();
    let timeout_interval = flags.get_one::<u32>("timeout").unwrap_or(&30).to_owned();

    // initialize logging
    let log_path = format!("{}/{}", &log_dir, log_filename);

    fs::create_dir_all(log_dir).or_else(|e| Err(io::Error::new(io::ErrorKind::Other, e)))?;

    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .or_else(|e| Err(io::Error::new(io::ErrorKind::Other, e)))?;

    let log_config = SimpleLogConfig::default();

    CombinedLogger::init(vec![
        TermLogger::new(
            log_level,
            log_config.clone(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
        WriteLogger::new(log_level, log_config, log_file),
    ])
    .or_else(|e| Err(io::Error::new(io::ErrorKind::Other, e)))?;

    // initialize broker
    let config = Config::new(ip, port, max_retries, retry_interval, timeout_interval);

    match Broker::run(config).await {
        Ok(_) => Ok(()),
        Err(Error::TokioErr(e)) => Err(e),
        Err(err) => {
            panic!("MQTT protocol error occurred: {:?}", err);
        }
    }
}
