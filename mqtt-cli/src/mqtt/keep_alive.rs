use std::sync::mpsc;
use std::thread;
use std::time;

use super::MqttContext;

#[derive(Debug)]
pub enum WakeReason {
    Reset, // reset the keep-alive ping timer (some command was manually run)
}

impl std::fmt::Display for WakeReason {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            WakeReason::Reset => write!(f, "Reset"),
        }
    }
}

/// Creates a thread that will periodically send a ping request based on the
/// keep alive interval. Call this function on connect if the keep alive
/// is non-zero to comply with server expectations of the client.
pub fn keep_alive(duration: time::Duration, context: &mut MqttContext) {
    let (tx, rx) = mpsc::channel::<WakeReason>();

    context.keep_alive = Some(thread::spawn(move || {
        loop {
            match rx.recv_timeout(duration) {
                Ok(msg) => { println!("received message {}", msg); },
                Err(ref err) => {
                    match err {
                        mpsc::RecvTimeoutError::Timeout => {
                            println!("timed out!");
                            // TODO: shell.parse_and_execute("ping", state, context);
                        },
                        mpsc::RecvTimeoutError::Disconnected => {
                            break;
                        }
                    }
                },
            }
        }
    }));
    context.keep_alive_tx = Some(tx);
}
