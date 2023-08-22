use bytes::Bytes;
use clap::{arg, command, value_parser, ArgAction};
use futures::{SinkExt, StreamExt};
use mqttrs::*;
use mqttrs_1::broker;
use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::{BytesCodec, Framed};

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

    // TODO: implement broker functionality and uncomment
    //let broker = broker::Broker::new(broker::Config::new(ip, port));
    //broker.start().await?;

    println!("Starting MQTT broker on {}:{}...", ip, port);

    // listen on tcp port using tokio (use 0.0.0.0 to listen on all addresses)
    let address = SocketAddr::new(ip, port);
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
                        Packet::Subscribe(subscribe) => {
                            let num_topics = subscribe.topics.len();
                            if num_topics == 0 {
                                println!("Received subscribe packet that does not have any topics. Ignoring...");
                                continue;
                            }

                            println!("Received subscription request for the following topics:\n");
                            for topic in subscribe.topics.iter() {
                                println!("  {}", topic.topic_path);
                            }
                            println!("\n");

                            // TODO: the rest of this function sends a dummy message to a random
                            // topic followed by subscribing client. This is only for testing
                            // purposes.

                            for _ in 0..10 {
                                std::thread::sleep(std::time::Duration::from_secs(5));

                                let mut rng = StdRng::from_entropy();
                                let rand_idx = rng.next_u32() as usize;
                                let topic = &subscribe.topics[rand_idx % num_topics].topic_path;

                                println!("Publishing dummy message to {}...", topic);

                                let pkt = Packet::Publish(Publish {
                                    dup: false,
                                    qospid: mqttrs::QosPid::AtMostOnce,
                                    retain: false,
                                    topic_name: topic,
                                    payload: "hello from broker".as_bytes(),
                                });
                                drop(rng);

                                let buf_sz = if let mqttrs::Packet::Publish(ref publish) = pkt {
                                    std::mem::size_of::<mqttrs::Publish>()
                                        + std::mem::size_of::<u8>() * publish.payload.len()
                                } else {
                                    0
                                };
                                let mut buf = vec![0u8; buf_sz];

                                let encoded = encode_slice(&pkt, &mut buf);
                                assert!(encoded.is_ok());

                                let encoded = Bytes::from(buf);
                                let _res = framed.send(encoded).await;
                            }

                            println!("Ending dummy message transmission to subscriber.");
                        }
                        Packet::Unsubscribe(unsubscribe) => {
                            println!("Received unsubscribe request for the following topics:\n");
                            for ref topic in unsubscribe.topics.iter() {
                                println!("  {}", topic);
                            }
                            println!("\n");

                            // send UNSUBACK
                            // TODO: implement pid handling
                            let pkt = Packet::Unsuback(mqttrs::Pid::new());
                            let mut buf = vec![
                                0u8;
                                std::mem::size_of::<Packet>()
                                    + std::mem::size_of::<mqttrs::Pid>()
                            ];

                            let encoded = encode_slice(&pkt, &mut buf);
                            assert!(encoded.is_ok());

                            let encoded = Bytes::from(buf);
                            let _res = framed.send(encoded).await;

                            // TODO: implement unsubscribe
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
