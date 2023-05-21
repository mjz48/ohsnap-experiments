use mqttrs::{clone_packet, decode_slice, Packet};
use std::io::{self, BufRead, BufReader};
use std::net::TcpStream;

pub fn read_into_buf(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut reader = BufReader::new(stream);
    let mut received: Vec<u8> = reader.fill_buf()?.to_vec();
    reader.consume(received.len());

    let mut ret_buf: Vec<u8> = received.clone();
    clone_packet(&mut received as &mut [u8], &mut ret_buf as &mut [u8])?;
    return Ok(ret_buf);
}

pub fn read_and_decode<'a>(stream: &mut TcpStream, buf: &'a mut Vec<u8>) -> io::Result<Packet<'a>> {
    let mut reader = BufReader::new(stream);
    *buf = reader.fill_buf()?.to_vec();
    reader.consume(buf.len());

    match decode_slice(buf)? {
        Some(pkt) => Ok(pkt),
        None => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "unable to decode incoming data",
        )),
    }
}
