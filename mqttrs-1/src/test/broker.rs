#[cfg(test)]
mod broker {
    use crate::{
        broker::{config::Config, Broker},
        mqtt::{self, Packet},
        test::fixtures::Client,
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

        let client =
            Client::new(SocketAddr::new(ip, port)).expect("test::client::Client create failed.");

        client
            .send_packet(&Packet::Pingreq)
            .await
            .expect("Could not send mqtt packet to broker.");

        // make sure the broker closes the connection
        assert!(
            client.get_framed_mut().next().await.is_none(),
            "Expected broker to close connection."
        );
    }
}
