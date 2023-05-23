use crate::cli::spec;

pub use connect::*;
pub use ping::*;
pub use publish::*;
pub use subscribe::*;

pub mod connect;
pub mod ping;
pub mod publish;
pub mod subscribe;

/// Quit the cli shell.
pub fn exit<Context: std::marker::Send>() -> spec::Command<Context> {
    spec::Command::build("exit")
        .set_help("Quit the command line interface.")
        .set_callback(|_command, _shell, _state, _context| Ok(spec::ReturnCode::Abort))
}

/// Print the cli shell help message.
pub fn help<Context: std::marker::Send>() -> spec::Command<Context> {
    spec::Command::build("help")
        .set_help("Print this help message")
        .set_callback(|_command, shell, _state, _context| {
            println!("{}", shell.help());
            Ok(spec::ReturnCode::Ok)
        })
}
