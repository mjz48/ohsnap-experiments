use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use mqttrs::*;
use std::error::Error;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::{BytesCodec, Framed};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("Starting MQTT broker...");

    // listen on tcp port using tokio (use 0.0.0.0 to listen on all addresses)
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 1883);
    let listener = TcpListener::bind(address).await?;

    loop {
        // TODO: need to handle connection failure
        let (stream, _addr) = listener.accept().await.unwrap();
        println!("New connection: {}", address);

        tokio::spawn(async move {
            handle_client(stream).await;
        });
    }
}

async fn handle_client(stream: TcpStream) {
    println!("New client thread spawned");

    let mut framed = Framed::new(stream, BytesCodec::new());

    // this should be the MQTT client's connect packet
    let packet = framed.next().await;
    println!("NEXT PACKET: {:#?}", packet);

    // send CONACK to client, which is expecting that
    let conack = Bytes::from(vec![32u8, 2, 0, 0]); // CONACK packet as a series of bytes
    println!("CONACK: {:#?}", conack);
    let _res = framed.send(conack).await;

    loop {
        match framed.next().await {
            Some(Ok(bytes)) => {
                match decode_slice(&bytes as &[u8]) {
                    Ok(Some(pkt)) => match pkt {
                        Packet::Pingreq => {
                            println!("Ping - Pong!");

                            // (send back a PINGRESP to keep connection alive)
                            let ping_resp = Bytes::from(vec![208u8, 0]);
                            let _res = framed.send(ping_resp).await;
                        }
                        Packet::Publish(publish) => {
                            let payload_str =
                                if let Ok(s) = String::from_utf8(publish.payload.to_vec()) {
                                    s
                                } else {
                                    eprintln!("Could not convert payload to string.");
                                    "".into()
                                };
                            let topic_str = publish.topic_name.to_owned();

                            println!("Received publish for topic '{}'...", topic_str);
                            println!("Message contents: '{}'...", payload_str);

                            // TODO: implement message publishing to all subscribed clients.
                        }
                        _ => {
                            println!("Received packet: {:?}", pkt);
                        }
                    },
                    Ok(None) => {
                        println!("Received empty packet.");
                        continue;
                    }
                    Err(err) => {
                        eprintln!("Unable to decode received packet: {:?}", err);
                        continue;
                    }
                }
            }
            pkt => {
                println!("Received an invalid packet: {:#?}", pkt);
                break;
            }
        }
    }
}
