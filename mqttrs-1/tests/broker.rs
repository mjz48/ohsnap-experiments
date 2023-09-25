/// Broker related tests. Basically these are integration tests. Most of these
/// tests can be matched up to a specific "Server MUST ..." statement in the
/// MQTT spec.
mod broker {
    use mqttrs_1::{
        mqtt::{self, Packet},
        test::{fixtures::Client, setup},
    };
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    /// Upon creating a tcp connection between broker and host, the first
    /// packet MUST be a connect packet. Otherwise, this is considered a
    /// protocol violation and the server must close the connection.
    ///
    /// [MQTT-3.1.0-1]
    /// http://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718133
    #[tokio::test]
    async fn non_connect_pkt_on_tcp_connection() {
        let ip = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));
        let port = setup::get_port().unwrap();

        setup::broker(ip, port.0, 0, 0, 30).await;

        let mut client = Client::new(SocketAddr::new(ip, port.0))
            .await
            .expect("test::Client create failed.");

        client.send(&Packet::Pingreq).await.unwrap();

        // make sure the broker closes the connection
        assert!(client.expect_stream_closed().await.is_ok());
    }

    /// Once connection has been established, the server MUST process a second
    /// CONNECT Packet sent from the Client as a protocol violation and
    /// disconnect the Client.
    ///
    /// [MQTT-3.1.0-2]
    /// http://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718133
    #[tokio::test]
    async fn two_connect_pkts_seen() {
        let ip = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));
        let port = setup::get_port().unwrap();

        setup::broker(ip, port.0, 0, 0, 30).await;

        let mut client = Client::new(SocketAddr::new(ip, port.0))
            .await
            .expect("test::Client create failed.");

        let connect = Packet::Connect(mqtt::Connect {
            clean_session: true,
            client_id: "test-client".into(),
            keep_alive: 0,
            last_will: None,
            protocol: mqtt::Protocol::MQTT311,
            username: None,
            password: None,
        });

        client.send(&connect).await.unwrap();
        assert!(client.expect_connack().await.is_ok());

        client.send(&connect).await.unwrap();
        assert!(client.expect_stream_closed().await.is_ok());
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
