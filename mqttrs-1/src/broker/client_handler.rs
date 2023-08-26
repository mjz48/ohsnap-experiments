use bytes::{Bytes, BytesMut};
use futures::{SinkExt, StreamExt};
use mqttrs::*;
use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};
use tokio::net::TcpStream;
use tokio_util::codec::{BytesCodec, Framed};

pub struct ClientHandler {
    framed: Framed<TcpStream, BytesCodec>,
}

impl ClientHandler {
    pub fn new(stream: TcpStream) -> ClientHandler {
        ClientHandler {
            framed: Framed::new(stream, BytesCodec::new()),
        }
    }

    pub async fn run(&mut self) {
        println!("client task spawned");

        // this should be the MQTT client's connect packet
        let packet = self.framed.next().await;
        println!("NEXT PACKET: {:#?}", packet);

        // send CONACK to client, which is expecting that
        let conack = Bytes::from(vec![32u8, 2, 0, 0]); // CONACK packet as a series of bytes
        println!("CONACK: {:#?}", conack);
        let _res = self.framed.send(conack).await;

        loop {
            match self.framed.next().await {
                Some(Ok(bytes)) => {
                    let new_bytes = bytes.clone();

                    match self.handle_packet(new_bytes).await {
                        Err(err) => {
                            eprintln!("Error during packet decode: {:?}", err);
                        }
                        _ => (),
                    }
                }
                pkt => {
                    println!("Received an invalid packet: {:#?}", pkt);
                    break;
                }
            }
        }
    }

    async fn handle_packet(&mut self, buf: BytesMut) -> Result<(), Box<dyn std::error::Error>> {
        match decode_slice(&buf as &[u8]) {
            Ok(Some(pkt)) => match pkt {
                Packet::Connect(connect) => self.handle_connect(connect).await?,
                Packet::Connack(connack) => self.handle_connack(connack).await?,
                Packet::Publish(publish) => self.handle_publish(publish).await?,
                Packet::Puback(pid) => self.handle_puback(pid).await?,
                Packet::Pubrec(pid) => self.handle_pubrec(pid).await?,
                Packet::Pubrel(pid) => self.handle_pubrel(pid).await?,
                Packet::Pubcomp(pid) => self.handle_pubcomp(pid).await?,
                Packet::Subscribe(subscribe) => self.handle_subscribe(subscribe).await?,
                Packet::Suback(suback) => self.handle_suback(suback).await?,
                Packet::Unsubscribe(unsubscribe) => self.handle_unsubscribe(unsubscribe).await?,
                Packet::Unsuback(pid) => self.handle_unsuback(pid).await?,
                Packet::Pingreq => self.handle_pingreq().await?,
                Packet::Pingresp => self.handle_pingresp().await?,
                Packet::Disconnect => self.handle_disconnect().await?,
            },
            Ok(None) => (), // buf does not contain a complete packet (but nothing is wrong)
            Err(err) => {
                eprintln!("Unable to decode received packet: {:?}", err);
            }
        }

        Ok(())
    }

    async fn handle_connect(
        &mut self,
        _connect: mqttrs::Connect<'_>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    async fn handle_connack(
        &mut self,
        _connack: mqttrs::Connack,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    async fn handle_publish(
        &mut self,
        publish: mqttrs::Publish<'_>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let payload_str = if let Ok(s) = String::from_utf8(publish.payload.to_vec()) {
            s
        } else {
            // TODO: return this as an error instead
            eprintln!("Could not convert payload to string.");
            "".into()
        };
        let topic_str = publish.topic_name.to_owned();

        println!("Received publish for topic '{}'...", topic_str);
        println!("Message contents: '{}'...", payload_str);

        // TODO: implement message publishing to all subscribed clients.
        Ok(())
    }

    async fn handle_puback(&mut self, _pid: mqttrs::Pid) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    async fn handle_pubrec(&mut self, _pid: mqttrs::Pid) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    async fn handle_pubrel(&mut self, _pid: mqttrs::Pid) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    async fn handle_pubcomp(
        &mut self,
        _pid: mqttrs::Pid,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    async fn handle_subscribe(
        &mut self,
        subscribe: mqttrs::Subscribe,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let num_topics = subscribe.topics.len();
        if num_topics == 0 {
            println!("Received subscribe packet that does not have any topics. Ignoring...");
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
            let _res = self.framed.send(encoded).await;
        }

        Ok(())
    }

    async fn handle_suback(
        &mut self,
        _suback: mqttrs::Suback,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    async fn handle_unsubscribe(
        &mut self,
        unsubscribe: mqttrs::Unsubscribe,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("Received unsubscribe request for the following topics:\n");
        for ref topic in unsubscribe.topics.iter() {
            println!("  {}", topic);
        }
        println!("\n");

        // send UNSUBACK
        // TODO: implement pid handling
        let pkt = Packet::Unsuback(mqttrs::Pid::new());
        let mut buf = vec![0u8; std::mem::size_of::<Packet>() + std::mem::size_of::<mqttrs::Pid>()];

        let encoded = encode_slice(&pkt, &mut buf);
        assert!(encoded.is_ok());

        let encoded = Bytes::from(buf);
        let _res = self.framed.send(encoded).await;

        // TODO: implement unsubscribe
        Ok(())
    }

    async fn handle_unsuback(
        &mut self,
        _pid: mqttrs::Pid,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    async fn handle_pingreq(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // respond to ping requests; will keep the connection alive
        let ping_resp = mqttrs::Packet::Pingresp {};
        let mut buf = vec![0u8; 3];

        encode_slice(&ping_resp, &mut buf as &mut [u8])?;
        Ok(self.framed.send(Bytes::from(buf)).await?)
    }

    async fn handle_pingresp(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    async fn handle_disconnect(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
}
