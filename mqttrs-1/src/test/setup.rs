pub use portref::get_port;

use crate::broker::{config::Config, Broker};
use std::{net::IpAddr, time::Duration};
use tokio::time::sleep;

mod portref;

/// Instantiate a broker inside a test. This creates a config and then does a
/// short delay so that anything that is trying to bind to the connection
/// will not fail.
///
/// # Arguments
///
/// * `ip` - an IpAddr to have the broker listen on
/// * `port` - port for the broker to listen on
/// * `max_retries` - broker config for max retries
/// * `retry_timeout` - broker config for retry timeout
/// * `timeout_interval` - broker config for timeout interval
pub async fn broker(
    ip: IpAddr,
    port: u16,
    max_retries: u32,
    retry_interval: u32,
    timeout_interval: u32,
) {
    let config = Config::new(ip, port, max_retries, retry_interval, timeout_interval);

    tokio::spawn(async move {
        Broker::run(config)
            .await
            .expect("Error while running broker")
    });

    // wait for broker to bind tcp port
    sleep(Duration::from_millis(10)).await;
}
