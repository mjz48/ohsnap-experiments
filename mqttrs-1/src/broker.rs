use client_handler::ClientHandler;
use std::net::{IpAddr, SocketAddr};
use tokio::net::TcpListener;

pub mod client_handler;

pub struct Config {
    addr: SocketAddr,
}

impl Config {
    pub fn new(ip: IpAddr, port: u16) -> Config {
        Config {
            addr: SocketAddr::new(ip, port),
        }
    }
}

pub struct Broker {
    config: Config,
    tcp: Option<TcpListener>,
}

impl Broker {
    pub fn new(config: Config) -> Broker {
        Broker { config, tcp: None }
    }

    pub async fn start(mut self) -> tokio::io::Result<()> {
        println!(
            "Starting MQTT broker on {}:{}...",
            self.config.addr.ip(),
            self.config.addr.port()
        );

        self.tcp = Some(TcpListener::bind(self.config.addr).await?);

        loop {
            // TODO: need to handle connection failure
            let (stream, _addr) = self.tcp.as_ref().unwrap().accept().await.unwrap();
            println!("New connection: {}", self.config.addr);

            tokio::spawn(async move {
                match ClientHandler::new(stream).run().await {
                    Ok(()) => (),
                    Err(err) => {
                        eprintln!("Error during client operation: {:?}", err);
                    }
                }
            });
        }
    }
}
