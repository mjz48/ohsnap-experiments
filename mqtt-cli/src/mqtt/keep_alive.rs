use crate::cli::shell::{self, State};
use std::io;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time;

#[derive(Debug)]
pub enum Msg {
    Suspend, // keep the thread alive but don't send any shell commands
    Resume,  // resume normal behavior (call after suspend)
    Reset,   // reset the keep-alive ping timer (some command was manually run)
    Kill,    // kill the thread
}

impl std::fmt::Display for Msg {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Msg::Reset => write!(f, "Reset"),
            Msg::Suspend => write!(f, "Suspend"),
            Msg::Resume => write!(f, "Resume"),
            Msg::Kill => write!(f, "Kill"),
        }
    }
}

pub struct KeepAliveThreadContext {
    pub join_handle: JoinHandle<()>,
    pub keep_alive_tx: mpsc::Sender<Msg>,
}

/// Creates a thread that will periodically send a ping request based on the
/// keep alive interval. Call this function on connect if the keep alive
/// is non-zero to comply with server expectations of the client.
pub fn spawn_keep_alive_thread(
    duration: time::Duration,
    state: &mut State,
) -> Result<KeepAliveThreadContext, io::Error> {
    let (keep_alive_tx, keep_alive_rx) = mpsc::channel::<Msg>();

    // get a clone of the sender, so we don't need to keep state around
    let state_cmd_tx = match state.get(shell::STATE_CMD_TX.into()) {
        Some(shell::StateValue::Sender(tx)) => tx.clone(),
        Some(_) | None => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "command queue tx not found in shell state",
            ));
        }
    };

    let join_handle = thread::spawn(move || {
        let mut is_suspended = false;

        loop {
            match keep_alive_rx.recv_timeout(duration) {
                Ok(wake_reason) => match wake_reason {
                    Msg::Suspend => {
                        is_suspended = true;
                    }
                    Msg::Reset => {
                        continue;
                    }
                    Msg::Resume => {
                        is_suspended = false;
                    }
                    Msg::Kill => {
                        break;
                    }
                },
                Err(err) => match err {
                    mpsc::RecvTimeoutError::Timeout => {
                        if is_suspended {
                            continue;
                        }

                        // time to send out ping to keep the connection open
                        if let Err(error) = state_cmd_tx.send("ping".into()) {
                            eprintln!("{}", error);
                        }
                    }
                    mpsc::RecvTimeoutError::Disconnected => {
                        break;
                    }
                },
            }

            if let Err(ref err) = keep_alive_rx.recv_timeout(duration) {
                match err {
                    mpsc::RecvTimeoutError::Timeout => {
                        // time to send out ping to keep the connection open
                        if let Err(error) = state_cmd_tx.send("ping".into()) {
                            eprintln!("{}", error);
                        }
                    }
                    mpsc::RecvTimeoutError::Disconnected => {
                        break;
                    }
                }
            }
        }
    });

    Ok(KeepAliveThreadContext {
        join_handle,
        keep_alive_tx,
    })
}
