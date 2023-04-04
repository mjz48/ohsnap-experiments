use std::error::Error;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("Starting MQTT broker...");

    // listen on tcp port using tokio
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1883);
    let mut listener = TcpListener::bind(address).await?;

    loop {
        // TODO: need to handle connection failure
        let (stream, addr) = listener.accept().await.unwrap();
        println!("New connection: {}", address);

        tokio::spawn(async {
            handle_client().await;
        });
    }
}

async fn handle_client() {
    println!("New client thread spawned")
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
