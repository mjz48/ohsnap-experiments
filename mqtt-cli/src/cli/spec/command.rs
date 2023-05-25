use super::flag;
use crate::cli::command;
use crate::cli::shell;
use crate::cli::spec;
use lexical_sort::{natural_lexical_cmp, StringSort};
use std::collections::HashMap;
use std::error::Error;
use std::fmt;

pub mod error;

/// Value that is returned when Commands finish running. This is used to
/// perform post execute() actions. For instance, 'Abort' will cause the shell
/// to exit upon completion.
#[derive(Clone, Copy, Debug)]
pub enum ReturnCode {
    Ok,
    Abort,
}

/// Callback type, supply spec::Command with function pointer for execution contents
pub type Callback<Context> = fn(
    &command::Command<Context>,
    &shell::Shell<Context>,
    &mut shell::State,
    &mut Context,
) -> Result<ReturnCode, Box<dyn Error>>;

/// A group of spec::Command's hashed by command name
pub type CommandSet<Context> = HashMap<String, Command<Context>>;

/// Command specification. Each flag must be unique.
pub struct Command<Context: std::marker::Send> {
    name: String,
    description: String,
    flags: flag::FlagSet,
    usage: String,
    callback: Callback<Context>,
}

impl<Context: std::marker::Send> Command<Context> {
    pub fn new(
        name: &str,
        description: &str,
        flags: flag::FlagSet,
        usage: &str,
        callback: Callback<Context>,
    ) -> Command<Context> {
        Command {
            name: name.into(),
            description: description.into(),
            flags,
            usage: usage.into(),
            callback,
        }
    }

    pub fn build(name: &str) -> Command<Context> {
        Command {
            name: name.into(),
            description: "".into(),
            flags: flag::FlagSet::new(),
            usage: "".into(),
            callback: |_c, _sh, _st, _context| Ok(ReturnCode::Ok),
        }
    }

    pub fn add_flag(
        mut self,
        flag_name: &str,
        flag_short: char,
        arg_spec: spec::Arg,
        help: &str,
    ) -> Command<Context> {
        self.flags
            .insert(spec::Flag::new(flag_name, flag_short, arg_spec, help));
        self
    }

    pub fn set_description(mut self, description: &str) -> Command<Context> {
        self.description = description.into();
        self
    }

    pub fn set_usage(mut self, usage: &str) -> Command<Context> {
        self.usage = usage.into();
        self
    }

    pub fn set_callback(mut self, callback: Callback<Context>) -> Command<Context> {
        self.callback = callback;
        self
    }

    pub fn callback(&self) -> &Callback<Context> {
        &self.callback
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn flags(&self) -> &flag::FlagSet {
        &self.flags
    }

    pub fn help(&self) -> String {
        let usage = self
            .usage
            .replace("{$name}", self.name())
            .replace("{$flags}", &self.get_flags_usage());

        let mut help = format!("Usage: {}\n\n  {}", usage, self.description());
        if self.flags().len() > 0 {
            help += &format!("\n\nOptions:\n\n{}", self.get_flags_help("  "));
        }

        help
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    fn get_flags_usage(&self) -> String {
        let mut flags: Vec<&spec::Flag> = self.flags.iter().collect();
        flags.string_sort_unstable(natural_lexical_cmp);

        let mut usage = String::new();
        for flag in flags.iter() {
            let arg_text = flag.id().name().replace("-", "_");

            match *flag.arg_spec() {
                spec::Arg::None => {
                    usage += &format!("[-{}] ", flag.id().short());
                }
                spec::Arg::Optional => {
                    usage += &format!("[-{} [{}]] ", flag.id().short(), arg_text);
                }
                spec::Arg::Required => {
                    usage += &format!("[-{} {}] ", flag.id().short(), arg_text);
                }
            }
        }

        usage.trim().to_owned()
    }

    fn get_flags_help(&self, prefix: &str) -> String {
        let mut flags: Vec<&spec::Flag> = self.flags.iter().collect();
        flags.string_sort_unstable(natural_lexical_cmp);

        let mut id_width = 0;
        let _: Vec<&spec::Flag> = flags
            .iter()
            .map(|f| {
                let width = "-x, --".len() + f.id().name().len() + "  ".len();
                if width > id_width {
                    id_width = width;
                }
                *f
            })
            .collect();

        let mut help = String::new();
        for flag in flags.iter() {
            let mut id = format!("{}-{}, --{}", prefix, flag.id().short(), flag.id().name());
            let start_len = id.len();

            for _ in 0..(id_width - start_len) {
                id += " ";
            }

            id += &(format!("  {}\n", flag.help()));
            help += &id;
        }

        help
    }
}

impl<Context: std::marker::Send> fmt::Debug for Command<Context> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.name.fmt(f)?;
        self.description.fmt(f)?;
        self.flags.fmt(f)?;
        Ok(())
    }
}

impl<Context: std::marker::Send> AsRef<str> for Command<Context> {
    fn as_ref(&self) -> &str {
        self.name()
    }
}
