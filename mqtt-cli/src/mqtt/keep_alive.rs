use std::sync::mpsc;
use std::thread;
use std::time;

use crate::cli::shell::{self, State};
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
pub fn keep_alive(duration: time::Duration, state: &mut State, context: &mut MqttContext) {
    let (tx, rx) = mpsc::channel::<WakeReason>();

    // get a clone of the sender, so we don't need to keep state around
    let ch = if let Some(shell::StateValue::Sender(ch))
        = state.get(shell::STATE_CMD_TX.into()) { ch.clone() }
    else {
        eprintln!("keep_alive: command queue tx not found in shell state.");
        return;
    };

    context.keep_alive = Some((thread::spawn(move || {
        loop {
            if let Err(ref err) = rx.recv_timeout(duration) {
                match err {
                    mpsc::RecvTimeoutError::Timeout => {
                        // time to send out ping to keep the connection open
                        if let Err(error) = ch.send("ping".into()) {
                            eprintln!("{}", error);
                        }
                    },
                    mpsc::RecvTimeoutError::Disconnected => {
                        break;
                    }
                }
            }
        }
    }), tx));
}
