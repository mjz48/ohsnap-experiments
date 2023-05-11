use std::thread;
use std::time;

use super::MqttContext;

/// Creates a thread that will periodically send a ping request based on the
/// keep alive interval. Call this function on connect if the keep alive
/// is non-zero to comply with server expectations of the client.
pub fn keep_alive(duration: time::Duration, context: &mut MqttContext) {
    context.keep_alive = Some(thread::spawn(move || {
        loop {
            let timeout = spawn_heartbeat_interval_thread(duration);
            timeout.join().unwrap();
        }
    }));
}

fn spawn_heartbeat_interval_thread(duration: time::Duration) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        println!("\rKeep alive thread heartbeat");
        thread::sleep(duration);
    })
}
