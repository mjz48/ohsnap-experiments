#[cfg(test)]
mod broker {
    use crate::{
        broker::{config::Config, Broker},
        mqtt::{self, Packet},
        test::fixtures::client,
    };
    use futures::StreamExt;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn violation_on_non_connect_packet_first() {
        let ip = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));
        let port = 1883;

        // instantiate broker
        let config = Config::new(ip, port, 0, 0, 30);

        tokio::spawn(async move {
            Broker::run(config)
                .await
                .expect("Error while running broker")
        });

        // wait for broker to bind tcp port
        sleep(Duration::from_millis(200)).await;

        let mut framed = client::open_framed(SocketAddr::new(ip, port))
            .await
            .expect("Couldn't open tcp connection!");

        mqtt::send(&Packet::Pingreq, &mut framed)
            .await
            .expect("Could not send mqtt packet to broker.");

        // make sure the broker closes the connection
        assert!(
            framed.next().await.is_none(),
            "Expected broker to close connection."
        );
    }
}
