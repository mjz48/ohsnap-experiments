use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use std::error::Error;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::{Framed, BytesCodec};

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
        // hold on to the client
    }
}

// TODO: this is from mqttrs example (Delete or incorporate at some point)
//use bytes::BytesMut;
//use mqttrs::*;
//
//fn main() {
//    // Allocate write buffer
//    let mut buf = BytesMut::with_capacity(1024);
//    
//    // Encode an MQTT Connect packet
//    let pkt = Packet::Connect(
//        Connect {
//            protocol: Protocol::MQTT311,
//            keep_alive: 30,
//            client_id: "doc_client".into(),
//            clean_session: true,
//            last_will: None,
//            username: None,
//            password: None
//        }
//    );
//    
//    assert!(encode_slice(&pkt, &mut buf).is_ok());
//    assert_eq!(&buf[14..], "doc_client".as_bytes());
//    let mut encoded = buf.clone();
//    
//    // Decode one packet. The buffer will advance to the next packet.
//    assert_eq!(Ok(Some(pkt)), decode_slice(&mut buf));
//}    
