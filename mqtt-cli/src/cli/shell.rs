use super::command;
use super::command::flag;
use super::command::operand::{Operand, OperandList};
use super::spec;
use super::spec::command::error::UnknownCommandError;
use super::spec::flag::error::{FlagMissingArgError, UnknownFlagError};
use lexer::IntoArgs;
use lexical_sort::{natural_lexical_cmp, StringSort};
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io::{self, Write};
use std::sync::mpsc;
use std::thread;

pub use state::*;

pub mod state;

mod lexer;

pub const DEFAULT_PROMPT: &str = "#";
pub const STATE_CMD_TX: &str = "cmd_queue_tx";
pub const STATE_PROMPT_STRING: &str = "prompt";
pub const STATE_ON_RUN_COMMAND: &str = "on_run";

#[derive(Debug)]
pub struct CommandNotAvailableError(pub String);

impl Display for CommandNotAvailableError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "Error: {} is not available in this context.", self.0)
    }
}

impl Error for CommandNotAvailableError {}

/// Shell controls cli flow and contains information about Command configuration
pub struct Shell<Context: std::marker::Send> {
    commands: spec::CommandSet<Context>,
    enable_set: HashMap<String, bool>,
    help: String,
}

impl<Context: std::marker::Send> Shell<Context> {
    pub fn new(commands: spec::CommandSet<Context>, help: &str) -> Self {
        Shell {
            commands,
            enable_set: HashMap::new(),
            help: help.into(),
        }
    }

    /// Given a command name, query the shell command spec set to see if there
    /// is a matching spec. If there is, return a reference to it.
    pub fn find_command_spec(&self, command_name: &str) -> Option<&spec::Command<Context>> {
        self.get_enabled_commands()
            .get(&String::from(command_name))
            .and_then(|c| Some(*c))
    }

    /// print help function for this Shell
    pub fn help(&self) -> String {
        let mut help_str = self.help.clone() + "\n\n";

        let mut name_width = 0;
        let mut help_width = 0;

        let enabled_cmds = self.get_enabled_commands();

        let _tmp: Vec<()> = enabled_cmds
            .iter()
            .map(|e| {
                name_width = std::cmp::max(name_width, e.1.name().len() + 1);
                help_width = std::cmp::max(help_width, e.1.description().len() + 1);
            })
            .collect();

        // lexicographically sort commands by their names
        let mut commands: Vec<&spec::Command<Context>> =
            enabled_cmds.values().map(|v| *v).collect();
        commands.string_sort_unstable(natural_lexical_cmp);

        // do this to avoid having to pull in a formatting crate
        for c in commands.iter() {
            for idx in 0..name_width {
                if idx < name_width - c.name().len() {
                    help_str += " ";
                } else {
                    break;
                }
            }
            help_str = help_str + c.name() + "    " + c.description();

            for _ in 0..(help_width - c.description().len()) {
                help_str += " ";
            }
            help_str += "\n";
        }

        help_str
    }

    pub fn quit(&self) {
        // any "on_quit" actions should be run here
        println!("Goodbye.\n");
    }

    pub fn run(&mut self, mut state: State, mut context: Context) {
        let (cmd_queue_tx, cmd_queue_rx) = mpsc::channel::<String>();
        let (rsp_queue_tx, rsp_queue_rx) = mpsc::channel::<spec::ReturnCode>();

        // add cmd_queue_tx to shell state
        state.insert(
            STATE_CMD_TX.into(),
            StateValue::Sender(cmd_queue_tx.clone()),
        );

        // run command on start if defined
        if let Some(StateValue::String(on_run)) = state.get(STATE_ON_RUN_COMMAND) {
            if let Err(err) = cmd_queue_tx.send(on_run.into()) {
                eprintln!("{}", err);
            }
        }

        let sh_exec = std::sync::Arc::new(std::sync::Mutex::new(self));
        let sh_input = sh_exec.clone();

        let state_execution_thread = std::sync::Arc::new(std::sync::Mutex::new(state));
        let state_input_thread = state_execution_thread.clone();

        thread::scope(|s| {
            s.spawn(move || {
                // receives user input and then parses and executes commands
                'run: loop {
                    let cmd = match cmd_queue_rx.recv() {
                        Ok(c) => c,
                        Err(error) => {
                            eprintln!("{}", error);
                            return;
                        }
                    };
                    let mut state = match state_execution_thread.lock() {
                        Ok(guard) => guard,
                        Err(error) => {
                            eprintln!("{}", error);
                            return;
                        }
                    };

                    let mut shell = match sh_exec.lock() {
                        Ok(shell) => shell,
                        Err(err) => {
                            eprintln!("{}", err);
                            return;
                        }
                    };

                    shell.update_enable_set(&mut state, &mut context);

                    let res = match shell.parse_and_execute(&cmd, &mut state, &mut context) {
                        Ok(code) => code,
                        Err(error) => {
                            eprintln!("{}", error);
                            spec::ReturnCode::Ok
                        }
                    };
                    std::mem::drop(state);

                    if let spec::ReturnCode::Abort = res {
                        shell.quit();
                        std::mem::drop(shell);
                        break 'run;
                    }
                    std::mem::drop(shell);

                    if let Err(error) = rsp_queue_tx.send(res) {
                        eprintln!("{}", error);
                        break 'run;
                    }
                }
            });
            s.spawn(move || {
                // handles user input and passes it to execution thread (above)
                'run: loop {
                    let rsp = rsp_queue_rx.recv();
                    match rsp {
                        Ok(code) => {
                            if let spec::ReturnCode::Abort = code {
                                break 'run;
                            }
                        }
                        Err(error) => {
                            // channel has been destroyed, abort (this should not happen)
                            eprintln!("{}", error);
                            break 'run;
                        }
                    }

                    // add this loop here to drain the rest of the queue
                    let mut rx_iter = rsp_queue_rx.try_iter();
                    let mut rsp = rx_iter.next();
                    while rsp.is_some() {
                        match rsp.unwrap() {
                            spec::ReturnCode::Abort => {
                                break 'run;
                            }
                            _ => (),
                        }
                        rsp = rx_iter.next();
                    }

                    {
                        let state = state_input_thread.lock().unwrap();
                        let shell = sh_input.lock().unwrap();

                        print!("{} ", shell.make_shell_prompt(&state));
                        io::stdout().flush().unwrap();
                    }

                    let mut input = String::new();
                    io::stdin()
                        .read_line(&mut input)
                        .expect("failed to read line");

                    let mut quote_stack = vec![];
                    lexer::count_quotes(&input, &mut quote_stack);

                    while quote_stack.len() > 0 {
                        print!("quote> ");
                        io::stdout().flush().unwrap();

                        let mut multi_line_input = String::new();
                        io::stdin()
                            .read_line(&mut multi_line_input)
                            .expect("failed to read line");

                        lexer::count_quotes(&multi_line_input, &mut quote_stack);
                        input += &multi_line_input;
                    }

                    let input = input.trim();
                    if let Err(error) = cmd_queue_tx.send(input.into()) {
                        eprintln!("{}", error);
                        break 'run;
                    }

                    if input == "exit" {
                        break 'run;
                    }
                }
            });
        });
    }

    /// generate prompt string
    fn make_shell_prompt(&self, state: &State) -> String {
        match state.get(STATE_PROMPT_STRING) {
            Some(StateValue::String(s)) => format!("{}>", s).into(),
            Some(StateValue::RichString(cs)) => format!("{}>", cs).into(),
            _ => format!("{}>", String::from(DEFAULT_PROMPT)).into(),
        }
    }

    /// Take a string that is presumably a valid cli command and turn it into
    /// a command::Command
    pub fn parse<'a>(
        &'a self,
        input_text: &str,
    ) -> Result<Option<command::Command<'a, Context>>, Box<dyn Error>> {
        let tokens = input_text.try_into_args()?;
        if tokens.len() == 0 {
            // if the user hit enter and didn't type anything, we received an
            // empty string. This is not a bug. Just ignore and redisplay the
            // prompt.
            return Ok(None);
        }
        let command_name = tokens.first().unwrap();

        let command_spec = match self.find_command_spec(command_name) {
            Some(spec) => spec,
            None => {
                if self.commands.get(command_name).is_some() {
                    // the command exists, but it's not enabled
                    return Err(Box::new(CommandNotAvailableError(command_name.into())));
                }

                return Err(Box::new(UnknownCommandError(command_name.into())));
            }
        };

        let mut tokens = tokens.iter().skip(1).peekable();
        let mut command =
            command::Command::new(command_spec, flag::FlagSet::new(), OperandList::new());

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
                    }
                    spec::Arg::Required => {
                        if next.is_none() || spec::flag::is_flag(next.unwrap()) {
                            return Err(Box::new(FlagMissingArgError(flag_id)));
                        }
                        command::Arg::Required(tokens.next().unwrap().to_string())
                    }
                    _ => command::Arg::None,
                };

                // it is not an error to pass in the same flag multiple times a
                // later value should overwrite an earlier one
                command
                    .flags_mut()
                    .replace(flag::Flag::<'a>::new(&flag_spec, parsed_arg));
            } else {
                command.operands_mut().push(Operand::new(token));
            }
        }

        Ok(Some(command))
    }

    /// parse a user input string and run the resulting command or show error.
    /// This does parse() and then command.execute().
    fn parse_and_execute(
        &self,
        input_text: &str,
        state: &mut State,
        context: &mut Context,
    ) -> Result<spec::ReturnCode, Box<dyn Error>> {
        let c_opt = self.parse(input_text)?;
        if let Some(c) = c_opt {
            c.execute(self, state, context)
        } else {
            Ok(spec::ReturnCode::Ok)
        }
    }

    /// filter the shell's CommandSet by their enable callbacks
    fn update_enable_set(&mut self, state: &mut State, context: &mut Context) {
        for (name, cmd) in self.commands.iter() {
            self.enable_set.insert(
                name.into(),
                (cmd.enable_callback())(&cmd, &self, state, context),
            );
        }
    }

    fn get_enabled_commands(&self) -> HashMap<&String, &spec::Command<Context>> {
        self.commands
            .iter()
            .filter(|(name, _)| {
                if let Some(enable) = self.enable_set.get(*name) {
                    *enable
                } else {
                    false
                }
            })
            .collect()
    }
}
