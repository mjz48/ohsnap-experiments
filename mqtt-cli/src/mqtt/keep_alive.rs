use crate::cli::shell::{self, State};
use std::io;
use std::io::Write as ioWrite;
use std::net::TcpStream;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time;

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

pub struct KeepAliveThreadContext {
    pub join_handle: JoinHandle<()>,
    pub keep_alive_tx: mpsc::Sender<WakeReason>,
}

/// Creates a thread that will periodically send a ping request based on the
/// keep alive interval. Call this function on connect if the keep alive
/// is non-zero to comply with server expectations of the client.
pub fn spawn_keep_alive_thread(
    duration: time::Duration,
    state: &mut State,
) -> Result<KeepAliveThreadContext, io::Error> {
    let (keep_alive_tx, keep_alive_rx) = mpsc::channel::<WakeReason>();

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
        loop {
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

/// implement trait to wrap std::net::TcpStream
pub trait KeepAliveTcpStream {
    /// overrides TcpStream.write to reset keep alive timer
    fn write(&mut self, buf: &[u8], tx: Option<mpsc::Sender<WakeReason>>)
        -> std::io::Result<usize>;

    /// do a TcpStream write without resetting keep alive timer
    fn write_no_keep_alive(&mut self, buf: &[u8]) -> std::io::Result<usize>;
}

impl KeepAliveTcpStream for TcpStream {
    fn write(
        &mut self,
        buf: &[u8],
        tx: Option<mpsc::Sender<WakeReason>>,
    ) -> std::io::Result<usize> {
        if let Some(t) = tx {
            if let Err(error) = t.send(WakeReason::Reset) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::ConnectionAborted,
                    error,
                ));
            }
        }
        ioWrite::write(self, buf)
    }

    fn write_no_keep_alive(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        ioWrite::write(self, buf)
    }
}
