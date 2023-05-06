use std::collections::HashMap;
use std::error::Error;
use std::fmt;

use crate::command;
use crate::shell;
use super::flag;

pub mod error;

/// Callback ReturnCode. This is used to perform post execute() actions. For
/// instance, 'Abort' will cause the shell to exit upon completion.
pub enum ReturnCode {
    Ok,
    Abort,
}

/// Callback type, supply spec::Command with function pointer for execution contents
pub type Callback = fn(&command::Command, &shell::Shell, &mut shell::Context)
    -> Result<ReturnCode, Box<dyn Error>>;

/// A group of spec::Command's hashed by command name
pub type CommandSet = HashMap<String, Command>;

/// Command specification. Each flag must be unique.
pub struct Command {
    name: String,
    flags: flag::FlagSet,
    help: String,
    callback: Callback,
}

impl Command {
    pub fn new(
        name: &str,
        flags: flag::FlagSet,
        help: &str,
        callback: Callback
    ) -> Command {
        Command { name: name.into(), flags, help: help.into(), callback }
    }

    pub fn callback(&self) -> &Callback {
        &self.callback
    }

    pub fn flags(&self) -> &flag::FlagSet {
        &self.flags
    }

    pub fn help(&self) -> &str {
        &self.help
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Debug for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.name.fmt(f)?;
        self.flags.fmt(f)?;
        self.help.fmt(f)?;
        Ok(())
    }
}
