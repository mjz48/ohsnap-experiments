use crate::cli::spec;
use crate::cli::spec::command::error::UnknownCommandError;

pub use connect::*;
pub use disconnect::*;
pub use ping::*;
pub use publish::*;
pub use subscribe::*;
pub use unsubscribe::*;

pub mod connect;
pub mod disconnect;
pub mod ping;
pub mod publish;
pub mod subscribe;
pub mod unsubscribe;

/// Quit the cli shell.
pub fn exit<Context: std::marker::Send>() -> spec::Command<Context> {
    spec::Command::build("exit")
        .set_description("Quit the command line interface")
        .set_usage("{$name}")
        .set_callback(|_command, _shell, _state, _context| Ok(spec::ReturnCode::Abort))
}

/// Print the cli shell help message.
pub fn help<Context: std::marker::Send>() -> spec::Command<Context> {
    spec::Command::build("help")
        .set_description("Print this help message or print help for a specific command")
        .set_usage("{$name} [command_name]")
        .set_callback(|command, shell, _state, _context| {
            let cmd = command.operands().iter().next();

            if let Some(c) = cmd {
                let command_name = c.value().to_owned();
                let command_spec = match shell.find_command_spec(&command_name) {
                    Some(spec) => spec,
                    None => {
                        return Err(Box::new(UnknownCommandError(command_name)));
                    }
                };

                println!("{}", command_spec.help());
            } else {
                // if there's no command, print help for shell
                println!("{}", shell.help());
            }

            Ok(spec::ReturnCode::Ok)
        })
}
