pub use mqttrs::{
    Connack, ConnectReturnCode, Pid, Protocol, QoS, QosPid, Suback, Subscribe,
    SubscribeReturnCodes, Unsubscribe,
};

use std::hash::{Hash, Hasher};

/// struct representing Last Will message. Based on mqttrs crate.
#[derive(Debug, Clone, PartialEq)]
pub struct LastWill {
    pub topic: String,
    pub message: Vec<u8>,
    pub qos: QoS,
    pub retain: bool,
}

/// enum representing MQTT control packets. Based on mqttrs crate.
#[derive(Debug, Clone, PartialEq)]
pub enum Packet {
    Connack(Connack),
    Connect(Connect),
    Publish(Publish),
    Puback(Pid),
    Pubrec(Pid),
    Pubrel(Pid),
    Pubcomp(Pid),
    Subscribe(Subscribe),
    Suback(Suback),
    Unsubscribe(Unsubscribe),
    Unsuback(Pid),
    Pingreq,
    Pingresp,
    Disconnect,
}

/// struct representing data from Connect MQTT control packet.
#[derive(Debug, Clone, PartialEq)]
pub struct Connect {
    pub protocol: Protocol,
    pub keep_alive: u16,
    pub client_id: String,
    pub clean_session: bool,
    pub last_will: Option<LastWill>,
    pub username: Option<String>,
    pub password: Option<Vec<u8>>,
}

/// struct representing data from Publish MQTT control packet.
#[derive(Debug, Clone, PartialEq)]
pub struct Publish {
    pub dup: bool,
    pub qospid: QosPid,
    pub retain: bool,
    pub topic_name: String,
    pub payload: Vec<u8>,
}

/// struct representing a Subscribe topic. This is equivelant to mqttrs::SubscribeTopic
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscribeTopic {
    pub topic_path: String,
    pub qos: QoS,
}

impl Hash for SubscribeTopic {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.topic_path.hash(state);
    }
}

impl From<mqttrs::SubscribeTopic> for SubscribeTopic {
    fn from(other: mqttrs::SubscribeTopic) -> Self {
        SubscribeTopic {
            topic_path: other.topic_path,
            qos: other.qos,
        }
    }
}

impl From<SubscribeTopic> for String {
    fn from(other: SubscribeTopic) -> Self {
        other.topic_path
    }
}

impl From<String> for SubscribeTopic {
    fn from(other: String) -> Self {
        SubscribeTopic {
            topic_path: other,
            qos: QoS::AtMostOnce,
        }
    }
}

impl Packet {
    /// convert between mqtt::Packet and mqttrs::Packet. We can't do this through
    /// the `Into<T>` trait because that interface requires ownership of input and
    /// there is no way to link lifetimes together.
    pub fn into<'a>(pkt: &'a Packet) -> mqttrs::Packet<'a> {
        match pkt {
            Packet::Connack(connack) => mqttrs::Packet::Connack(*connack),
            Packet::Connect(connect) => mqttrs::Packet::Connect(mqttrs::Connect {
                protocol: connect.protocol,
                keep_alive: connect.keep_alive,
                client_id: &connect.client_id,
                clean_session: connect.clean_session,
                last_will: match connect.last_will {
                    Some(ref lw) => Some(mqttrs::LastWill {
                        topic: &lw.topic,
                        message: &lw.message,
                        qos: lw.qos,
                        retain: lw.retain,
                    }),
                    None => None,
                },
                username: match connect.username {
                    Some(ref username) => Some(username),
                    None => None,
                },
                password: match connect.password {
                    Some(ref password) => Some(&password),
                    None => None,
                },
            }),
            Packet::Publish(publish) => mqttrs::Packet::Publish(mqttrs::Publish {
                dup: publish.dup,
                qospid: publish.qospid,
                retain: publish.retain,
                topic_name: &publish.topic_name,
                payload: &publish.payload,
            }),
            Packet::Puback(pid) => mqttrs::Packet::Puback(*pid),
            Packet::Pubrec(pid) => mqttrs::Packet::Pubrec(*pid),
            Packet::Pubrel(pid) => mqttrs::Packet::Pubrel(*pid),
            Packet::Pubcomp(pid) => mqttrs::Packet::Pubcomp(*pid),
            Packet::Subscribe(subscribe) => mqttrs::Packet::Subscribe(mqttrs::Subscribe {
                pid: subscribe.pid,
                topics: subscribe.topics.clone(),
            }),
            Packet::Suback(suback) => mqttrs::Packet::Suback(mqttrs::Suback {
                pid: suback.pid,
                return_codes: suback.return_codes.clone(),
            }),
            Packet::Unsubscribe(unsubscribe) => mqttrs::Packet::Unsubscribe(mqttrs::Unsubscribe {
                pid: unsubscribe.pid,
                topics: unsubscribe.topics.clone(),
            }),
            Packet::Unsuback(pid) => mqttrs::Packet::Unsuback(*pid),
            Packet::Pingreq => mqttrs::Packet::Pingreq,
            Packet::Pingresp => mqttrs::Packet::Pingresp,
            Packet::Disconnect => mqttrs::Packet::Disconnect,
        }
    }
}

impl From<mqttrs::Packet<'_>> for Packet {
    fn from(pkt: mqttrs::Packet<'_>) -> Self {
        match pkt {
            mqttrs::Packet::Connack(connack) => Packet::Connack(connack),
            mqttrs::Packet::Connect(mqttrs::Connect {
                protocol,
                keep_alive,
                client_id,
                clean_session,
                last_will,
                username,
                password,
            }) => Packet::Connect(Connect {
                protocol,
                keep_alive,
                client_id: client_id.to_string(),
                clean_session,
                last_will: last_will.and_then(|lw| {
                    Some(LastWill {
                        topic: lw.topic.to_string(),
                        message: lw.message.to_vec(),
                        qos: lw.qos,
                        retain: lw.retain,
                    })
                }),
                username: username.and_then(|u| Some(u.to_string())),
                password: password.and_then(|p| Some(p.to_vec())),
            }),
            mqttrs::Packet::Publish(mqttrs::Publish {
                dup,
                qospid,
                retain,
                topic_name,
                payload,
            }) => Packet::Publish(Publish {
                dup,
                qospid,
                retain,
                topic_name: topic_name.to_string(),
                payload: payload.to_vec(),
            }),
            mqttrs::Packet::Puback(pid) => Packet::Puback(pid),
            mqttrs::Packet::Pubrec(pid) => Packet::Pubrec(pid),
            mqttrs::Packet::Pubrel(pid) => Packet::Pubrel(pid),
            mqttrs::Packet::Pubcomp(pid) => Packet::Pubcomp(pid),
            mqttrs::Packet::Subscribe(subscribe) => Packet::Subscribe(subscribe),
            mqttrs::Packet::Suback(suback) => Packet::Suback(suback),
            mqttrs::Packet::Unsubscribe(unsubscribe) => Packet::Unsubscribe(unsubscribe),
            mqttrs::Packet::Unsuback(pid) => Packet::Unsuback(pid),
            mqttrs::Packet::Pingreq => Packet::Pingreq,
            mqttrs::Packet::Pingresp => Packet::Pingresp,
            mqttrs::Packet::Disconnect => Packet::Disconnect,
        }
    }
}
