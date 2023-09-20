//! # mqttrs-1
//!
//! This is a rust-only implementation of an MQTT broker that uses the rust
//! async library to support many client connections simultaneously.
//!
//! The goal of this crate is to serve as a proof-of-concept MQTT broker for
//! the OHSNAP project. This crate is also meant to be compatible with OpenBSD.
pub mod broker;
pub mod error;
pub mod mqtt;

pub mod test;
