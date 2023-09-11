use crate::error::{Error, Result};
use bytes::Bytes;
use futures::SinkExt;
use mqttrs::{encode_slice, Connack, Connect, Packet, Pid, Subscribe, Unsubscribe};
use tokio::net::TcpStream;
use tokio_util::codec::{BytesCodec, Framed};

/// Given an mqttrs::Packet reference, allocate a byte buffer with the packet
/// contents encoded into it. This function automatically calculates the size
/// of the provided packet and returns a vector of the correct size.
///
/// # Arguments
///
/// * `pkt` - mqttrs::Packet reference to encode
///
/// # Errors:
///
/// This function may throw the following errors:
///
/// * EncodeFailed
///
pub fn encode(pkt: &Packet) -> Result<Vec<u8>> {
    let sz = std::mem::size_of::<Packet>()
        + match pkt {
            Packet::Connack(_) => std::mem::size_of::<Connack>(),
            Packet::Connect(_) => std::mem::size_of::<Connect>(),
            Packet::Publish(publish) => {
                std::mem::size_of::<mqttrs::Publish>()
                    + std::mem::size_of::<u8>() * publish.payload.len()
            }
            Packet::Puback(_)
            | Packet::Pubrec(_)
            | Packet::Pubrel(_)
            | Packet::Pubcomp(_)
            | Packet::Unsuback(_) => std::mem::size_of::<Pid>(),
            Packet::Subscribe(subscribe) => {
                let mut len = std::mem::size_of::<Subscribe>();
                for t in &subscribe.topics[..] {
                    len += t.topic_path.len();
                }
                len
            }
            Packet::Suback(suback) => {
                std::mem::size_of::<mqttrs::Suback>()
                    + std::mem::size_of::<mqttrs::SubscribeReturnCodes>()
                        * suback.return_codes.len()
            }
            Packet::Unsubscribe(unsubscribe) => {
                let mut len = std::mem::size_of::<Unsubscribe>();
                for t in &unsubscribe.topics[..] {
                    len += t.len();
                }
                len
            }
            Packet::Pingreq | Packet::Pingresp | Packet::Disconnect => 0,
        };
    let mut buf = vec![0u8; sz];

    encode_slice(pkt, &mut buf as &mut [u8]).or_else(|e| {
        Err(Error::EncodeFailed(format!(
            "Unable to encode packet: {:?}",
            e
        )))
    })?;

    Ok(buf)
}

/// Given an mqttrs::Packet reference, send it across a framed tcp connection.
/// This function creates and deletes a buffer to encode the packet. (This should
/// be known to the user to decide if the extra memory allocation is acceptable.)
///
/// # Arguments:
///
/// * `pkt` - an mqttrs::Packet reference to send
/// * `framed` - a tokio Framed tcp channel to send on
///
/// # Errors:
///
/// This function may throw the following errors:
///
/// * EncodeFailed
/// * PacketSendFailed
pub async fn send(pkt: &Packet<'_>, framed: &mut Framed<TcpStream, BytesCodec>) -> Result<()> {
    let buf = encode(pkt)?;

    Ok(framed.send(Bytes::from(buf)).await.or_else(|e| {
        Err(Error::PacketSendFailed(format!(
            "Unable to send packet: {:?}",
            e
        )))
    })?)
}
