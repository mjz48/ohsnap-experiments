/// Broker related tests. Basically these are integration tests. Most of these
/// tests can be matched up to a specific "Server MUST ..." statement in the
/// MQTT spec.
mod broker {
    use futures::StreamExt;
    use mqttrs_1::{
        broker::{config::Config, Broker},
        mqtt::Packet,
        test::fixtures::Client,
    };
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn non_connect_pkt_on_tcp_connection() {
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

        let mut client = Client::new(SocketAddr::new(ip, port))
            .await
            .expect("test::client::Client create failed.");

        client
            .send(&Packet::Pingreq)
            .await
            .expect("Could not send mqtt packet to broker.");

        // make sure the broker closes the connection
        assert!(
            client.get_framed_mut().next().await.is_none(),
            "Expected broker to close connection."
        );
    }

    //#[tokio::test]
    //async fn broker_supports_mqtt3() {
    //    panic!("Implement me!");
    //}

    //#[tokio::test]
    //async fn broker_does_not_support_mqtt5() {
    //    panic!("Implement me!");
    //}

    //#[tokio::test]
    //async fn broker_closes_connection_on_connect_reserved_set() {
    //    panic!("Implement me!");
    //}

    //#[tokio::test]
    //async fn broker_retain_messages_if_retain_is_set() {
    //    panic!("Implement me!");
    //}
}
