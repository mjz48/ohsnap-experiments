pub use packet::{
    Connack, Connect, ConnectReturnCode, LastWill, Packet, Pid, Protocol, Publish, QoS, QosPid,
    Suback, Subscribe, SubscribeReturnCodes, Unsubscribe,
};

use crate::error::{Error, Result};
use bytes::Bytes;
use bytes::BytesMut;
use futures::SinkExt;
use mqttrs::{decode_slice, encode_slice, Packet as MqttrsPacket};
use tokio::net::TcpStream;
use tokio_util::codec::{BytesCodec, Framed};

pub mod packet;

/// Given a byte array from tcp connection, decode into MQTT packet. this
/// function will only ever return at most one MQTT packet.
///
/// # Arguments
///
/// * `buf` - reference to byte array possibly containing MQTT packet
///
/// # Errors
///
/// This function may return the following errors:
///
///     * InvalidPacket
pub fn decode(buf: &BytesMut) -> Result<Option<Packet>> {
    match decode_slice(buf as &[u8]) {
        Ok(res) => Ok(res.and_then(|pkt| Some(Packet::from(pkt)))),
        Err(err) => Err(Error::InvalidPacket(format!(
            "Unable to decode packet: {:?}",
            err
        ))),
    }
}

/// Given an mqtt::Packet reference, allocate a byte buffer with the packet
/// contents encoded into it. This function automatically calculates the size
/// of the provided packet and returns a vector of the correct size.
///
/// # Arguments
///
/// * `pkt` - an mqtt::Packet to encode into a byte buffer
///
/// # Errors:
///
/// This function may throw the following errors:
///
/// * EncodeFailed
///
pub fn encode(pkt: &Packet) -> Result<Vec<u8>> {
    let pkt = Packet::into(pkt);
    let sz = std::mem::size_of::<MqttrsPacket>()
        + match pkt {
            MqttrsPacket::Connack(_) => std::mem::size_of::<Connack>(),
            MqttrsPacket::Connect(_) => std::mem::size_of::<Connect>(),
            MqttrsPacket::Publish(ref publish) => {
                std::mem::size_of::<mqttrs::Publish>()
                    + std::mem::size_of::<u8>() * publish.payload.len()
            }
            MqttrsPacket::Puback(_)
            | MqttrsPacket::Pubrec(_)
            | MqttrsPacket::Pubrel(_)
            | MqttrsPacket::Pubcomp(_)
            | MqttrsPacket::Unsuback(_) => std::mem::size_of::<Pid>(),
            MqttrsPacket::Subscribe(ref subscribe) => {
                let mut len = std::mem::size_of::<Subscribe>();
                for t in &subscribe.topics[..] {
                    len += t.topic_path.len();
                }
                len
            }
            MqttrsPacket::Suback(ref suback) => {
                std::mem::size_of::<mqttrs::Suback>()
                    + std::mem::size_of::<mqttrs::SubscribeReturnCodes>()
                        * suback.return_codes.len()
            }
            MqttrsPacket::Unsubscribe(ref unsubscribe) => {
                let mut len = std::mem::size_of::<Unsubscribe>();
                for t in &unsubscribe.topics[..] {
                    len += t.len();
                }
                len
            }
            MqttrsPacket::Pingreq | MqttrsPacket::Pingresp | MqttrsPacket::Disconnect => 0,
        };
    let mut buf = vec![0u8; sz];

    encode_slice(&pkt, &mut buf as &mut [u8]).or_else(|e| {
        Err(Error::EncodeFailed(format!(
            "Unable to encode packet: {:?}",
            e
        )))
    })?;

    Ok(buf)
}

/// Given an mqttrs::MqttrsPacket reference, send it across a framed tcp connection.
/// This function creates and deletes a buffer to encode the packet. (This should
/// be known to the user to decide if the extra memory allocation is acceptable.)
///
/// # Arguments:
///
/// * `pkt` - an mqtt::Packet reference to send
/// * `framed` - a tokio Framed tcp channel to send on
///
/// # Errors:
///
/// This function may throw the following errors:
///
/// * EncodeFailed
/// * MqttrsPacketSendFailed
pub async fn send(pkt: &Packet, framed: &mut Framed<TcpStream, BytesCodec>) -> Result<()> {
    let buf = encode(pkt)?;

    Ok(framed.send(Bytes::from(buf)).await.or_else(|e| {
        Err(Error::PacketSendFailed(format!(
            "Unable to send packet: {:?}",
            e
        )))
    })?)
}
