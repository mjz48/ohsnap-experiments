use tokio::sync::mpsc::Sender;

#[derive(Debug)]
pub enum BrokerMsg {
    ClientConnected {
        client: String,
        client_tx: Sender<BrokerMsg>,
    },
    ClientDisconnected {
        client: String,
    },
    Publish {
        client: String,
        // these fields are from mqttrs::Publish
        dup: bool,
        //qospid: QosPid // TODO: implement
        retain: bool,
        topic_name: String,
        payload: Vec<u8>,
    },
    Subscribe {
        client: String,
        // these fields are from mqttrs::Subscribe
        //pid: Pid // TODO: implement
        topics: Vec<String>, // TODO: this is originally SubscribeTopics type, which has QoS informatino
    },
    Unsubscribe {
        client: String,
        //pid: Pid // TODO: implement
        topics: Vec<String>,
    },
}
