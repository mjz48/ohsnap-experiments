use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Write as fmt_Write};
use std::io::{self, Write};

use super::command;
use super::command::flag;
use super::command::operand::{Operand, OperandList};
use super::spec;
use super::spec::command::error::UnknownCommandError;
use super::spec::flag::error::{FlagMissingArgError, UnknownFlagError};

const DEFAULT_PROMPT: &str = "#";
const CONTEXT_PROMPT_STRING: &str = "prompt";
const CONTEXT_ON_RUN_COMMAND: &str = "on_run";

/// Data structure to contain shared variables that persist between commands.
/// Considered shell session state.
pub type Context = HashMap<String, String>;

/// Shell controls cli flow and contains information about Command configuration
pub struct Shell {
    commands: spec::CommandSet,
    help: String,
}

impl Shell {
    pub fn new(commands: spec::CommandSet, help: &str) -> Self {
        Shell { commands, help: help.into() }
    }

    /// Given a command name, query the shell command spec set to see if there
    /// is a matching spec. If there is, return a reference to it.
    pub fn find_command_spec(&self, command_name: &str) -> Option<&spec::Command> {
        self.commands.get(command_name)
    }

    /// print help function for this Shell
    pub fn help(&self) -> String {
        let mut help_str = format!("{}\n\n", self.help);

        let mut name_width = 0;
        let mut help_width = 0;

        let _tmp: Vec<()> = self.commands.iter().map(|e| {
            name_width = std::cmp::max(name_width, e.1.name().len() + 1);
            help_width = std::cmp::max(help_width, e.1.help().len() + 1);
        }).collect();

        // do this to avoid having to pull ina formatting crate
        for (_, c) in self.commands.iter() {
            for idx in 0..name_width {
                if idx < name_width - c.name().len() {
                    write!(help_str, "{}", " ").unwrap();
                } else {
                    break;
                }
            }
            write!(help_str, "{}    {}", c.name(), c.help()).unwrap();

            for _ in 0..(help_width - c.help().len()) {
                write!(help_str, "{}", " ").unwrap();
            }
            write!(help_str, "\n").unwrap();
        }

        help_str
    }

    pub fn quit(&self) {
        // any "on_quit" actions should be run here
        println!("Goodbye.\n");
    }

    pub fn run(&self, context: &mut Context) {
        let on_run_command = context.get(CONTEXT_ON_RUN_COMMAND)
            .unwrap_or(&String::from("")).clone();

        match self.parse_and_execute(&on_run_command, context) {
            Ok(code) => {
                if let spec::command::ReturnCode::Abort = code {
                    return;
                }
            },
            Err(error) => { eprintln!("{}", error); }
        }

        'run: loop {
            print!("{} ", self.make_shell_prompt(&(*context)));
            io::stdout().flush().unwrap();

            let mut input = String::new();
            io::stdin()
                .read_line(&mut input)
                .expect("failed to read line");
            let input = input.trim();

            match self.parse_and_execute(input, context) {
                Ok(code) => {
                    if let spec::command::ReturnCode::Abort = code {
                        self.quit();
                        break 'run;
                    }
                },
                Err(error) => { eprintln!("{}", error); }
            }
        }
    }

    /// generate prompt string
    fn make_shell_prompt(&self, context: &Context) -> String {
        let mut prompt_string = String::from(DEFAULT_PROMPT);
        if let Some(s) = context.get(CONTEXT_PROMPT_STRING) {
            prompt_string = s.clone();
        }

        format!("{}>", prompt_string).into()
    }

    /// Given a user entered command string, extract the command name (which is
    /// going to be the first argument separated by whitespace).
    fn extract_command_name<'a>(&self, input_text: &'a str) -> Option<&'a str> {
        input_text.split_whitespace().next()
    }

    /// Take a string that is presumably a valid cli command and turn it into
    /// a command::Command
    pub fn parse<'a>(&'a self, input_text: &str)
        -> Result<Option<command::Command<'a>>, Box<dyn Error>> {
        let command_name = match self.extract_command_name(input_text) {
            Some(name) => { name },
            None => {
                // what seems to have happened here is that the user hit "enter"
                // and didn't type in anything, so we received an empty string.
                // This is not a bug, just ignore and redisplay the prompt.
                return Ok(None);
            },
        };

        let command_spec = match self.find_command_spec(command_name) {
            Some(spec) => { spec },
            None => {
                return Err(Box::new(UnknownCommandError(command_name.into())));
            },
        };

        let mut tokens = input_text.split_whitespace().skip(1).peekable();
        let mut command = command::Command::new(
            command_spec,
            flag::FlagSet::new(),
            OperandList::new()
        );
    
        while tokens.peek().is_some() {
            let token = tokens.next().unwrap();
    
            if spec::flag::is_flag(&token) {
                let flag_id = spec::flag::extract(&token).unwrap();
                let flag_spec = spec::flag::query(&flag_id, command_spec.flags());
                if flag_spec.is_none() {
                    return Err(Box::new(UnknownFlagError(flag_id)));
                }
                let flag_spec = flag_spec.unwrap();
                    
                // check the argument spec and consume next token if necessary
                let next = tokens.peek();
                let parsed_arg = match flag_spec.arg_spec() {
                    spec::Arg::Optional => {
                        if next.is_none() || spec::flag::is_flag(next.unwrap()) {
                            continue;
                        }
                        command::Arg::Optional(Some(tokens.next().unwrap().to_string()))
                    },
                    spec::Arg::Required => {
                        if next.is_none() || spec::flag::is_flag(next.unwrap()) {
                            return Err(Box::new(FlagMissingArgError(flag_id)));
                        }
                        command::Arg::Required(tokens.next().unwrap().to_string())
                    },
                    _ => {
                        command::Arg::None
                    },
                };
    
                // it is not an error to pass in the same flag multiple times a
                // later value should overwrite an earlier one
                command.flags_mut().replace(flag::Flag::<'a>::new(&flag_spec, parsed_arg));
            } else {
                command.operands_mut().push(Operand::new(token));
            }
        }
    
        Ok(Some(command))
    }

    /// parse a user input string and run the resulting command or show error.
    /// This does parse_user_input() and then command.execute().
    fn parse_and_execute<'a>(&self, input_text: &str, context: &mut Context)
        -> Result<spec::command::ReturnCode, Box<dyn Error>> {
        match self.parse(input_text) {
            Ok(c_opt) => match c_opt {
                Some(command) => command.execute(&self, context),
                None => Ok(spec::command::ReturnCode::Ok),
            },
            Err(error) => Err(error),
        }
    }
}
