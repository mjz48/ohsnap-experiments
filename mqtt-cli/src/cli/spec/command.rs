use std::collections::HashMap;
use std::error::Error;
use std::fmt;

use crate::cli::command;
use crate::cli::shell;
use crate::cli::spec;
use super::flag;

pub mod error;

/// Value that is returned when Commands finish running. This is used to
/// perform post execute() actions. For instance, 'Abort' will cause the shell
/// to exit upon completion.
pub enum ReturnCode {
    Ok,
    Abort,
}

/// Callback type, supply spec::Command with function pointer for execution contents
pub type Callback<Context> = fn(
    &command::Command<Context>,
    &shell::Shell<Context>,
    &mut shell::State,
    &mut Context
) -> Result<ReturnCode, Box<dyn Error>>;

/// A group of spec::Command's hashed by command name
pub type CommandSet<Context> = HashMap<String, Command<Context>>;

/// Command specification. Each flag must be unique.
pub struct Command<Context> {
    name: String,
    flags: flag::FlagSet,
    help: String,
    callback: Callback<Context>,
}

impl<Context> Command<Context> {
    pub fn new(
        name: &str,
        flags: flag::FlagSet,
        help: &str,
        callback: Callback<Context>
    ) -> Command<Context> {
        Command { name: name.into(), flags, help: help.into(), callback }
    }

    pub fn build(name: &str) -> Command<Context> {
        Command {
            name: name.into(),
            flags: flag::FlagSet::new(),
            help: "".into(),
            callback: | _c, _sh, _st, _context | { Ok(ReturnCode::Ok) },
        }
    }

    pub fn add_flag(
        mut self,
        flag_name: &str,
        flag_short: char,
        arg_spec: spec::Arg,
        help: &str
    ) -> Command<Context> {
        self.flags.insert(spec::Flag::new(flag_name, flag_short, arg_spec, help));
        self
    }

    pub fn set_help(mut self, help: &str) -> Command<Context> {
        self.help = help.into();
        self
    }

    pub fn set_callback(mut self, callback: Callback<Context>) -> Command<Context> {
        self.callback = callback;
        self
    }

    pub fn callback(&self) -> &Callback<Context> {
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

impl<Context> fmt::Debug for Command<Context> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.name.fmt(f)?;
        self.flags.fmt(f)?;
        self.help.fmt(f)?;
        Ok(())
    }
}
