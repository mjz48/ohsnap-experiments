use crate::error::{Error, Result};
use lazy_static::lazy_static;
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

/// Get a new port number that is not being used by any existing TCP connection.
///
/// # Errors
///
/// This function may thorw the following errors:
///
/// * Error::CreateClientTaskFailed - when all ports in range are in use.
pub fn get_port() -> Result<PortRef> {
    let mut ports = PORTS_IN_USE.lock().unwrap();

    for port in 0..=(PORT_RANGE.end() - PORT_RANGE.start() + 1) {
        let port = (ports.0 + 1 + port) % PORT_RANGE.end();
        let port = if port < *PORT_RANGE.start() {
            port + PORT_RANGE.start()
        } else {
            port
        };

        if !ports.1.contains(&port) {
            ports.1.insert(port);
            ports.0 = port;
            return Ok(PortRef(port));
        }
    }

    return Err(Error::CreateClientTaskFailed(format!("Out of ports.")));
}

/// take a port number out of the ports in use. This is automatically called
/// when a PortRef is dropped.
fn drop_port(port: u16) {
    let mut ports = PORTS_IN_USE.lock().unwrap();
    ports.1.remove(&port.into());
}

lazy_static! {
    static ref PORT_RANGE: std::ops::RangeInclusive<u16> = 1883..=2883;
    static ref PORTS_IN_USE: Arc<Mutex<(u16, HashSet<u16>)>> =
        Arc::new(Mutex::new((*PORT_RANGE.start() - 1, HashSet::new())));
}

/// Port number wrapper. Contains special code to drop from static ports data
/// when the wrapper goes out of scope.
#[derive(Hash)]
pub struct PortRef(pub u16);

impl std::fmt::Display for PortRef {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Drop for PortRef {
    fn drop(&mut self) {
        drop_port(self.0);
    }
}

impl std::ops::Deref for PortRef {
    type Target = u16;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for PortRef {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl PartialEq for PortRef {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl Eq for PortRef {}

impl From<u16> for PortRef {
    fn from(p: u16) -> Self {
        PortRef(p)
    }
}

impl From<PortRef> for u16 {
    fn from(pr: PortRef) -> Self {
        pr.0
    }
}
