use crate::cli::shell::{self, Shell};
use crate::cli::spec;
use std::error::Error;
use std::fmt::{self, Debug, Formatter};

pub use arg::Arg;

pub mod arg;
pub mod flag;
pub mod operand;

/// Keep track of an instance of an executable command. This differs from
/// spec::Command because this contains an actual list of parsed Flags
/// and Operands that have values.
pub struct Command<'a, Context: std::marker::Send> {
    spec: &'a spec::Command<Context>,
    flags: flag::FlagSet<'a>,
    operands: operand::OperandList,
}

impl<'a, Context: std::marker::Send> Command<'a, Context> {
    pub fn new(
        spec: &'a spec::Command<Context>,
        flags: flag::FlagSet<'a>,
        operands: operand::OperandList,
    ) -> Command<'a, Context> {
        Command {
            spec,
            flags,
            operands,
        }
    }

    pub fn spec(&self) -> &spec::Command<Context> {
        &self.spec
    }

    pub fn execute(
        &self,
        shell: &Shell<Context>,
        state: &mut shell::State,
        context: &mut Context,
    ) -> Result<spec::ReturnCode, Box<dyn Error>> {
        (self.spec.callback())(&self, shell, state, context)
    }

    pub fn flags(&self) -> &flag::FlagSet<'a> {
        &self.flags
    }

    pub fn get_flag(&self, query: spec::flag::Query) -> Option<&flag::Flag> {
        flag::query(&query, &self.flags)
    }

    pub fn flags_mut(&mut self) -> &mut flag::FlagSet<'a> {
        &mut self.flags
    }

    pub fn operands(&self) -> &operand::OperandList {
        &self.operands
    }

    pub fn operands_mut(&mut self) -> &mut operand::OperandList {
        &mut self.operands
    }

    pub fn is_enabled(
        &self,
        shell: &Shell<Context>,
        state: &mut shell::State,
        context: &mut Context,
    ) -> bool {
        (self.spec().enable_callback())(self.spec(), shell, state, context)
    }
}

impl<'a, Context: std::marker::Send> Debug for Command<'a, Context> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.spec.fmt(f)?;
        self.flags.fmt(f)?;
        self.operands.fmt(f)?;
        Ok(())
    }
}
