use std::error::Error;

pub use arg::Arg;
use crate::cli::shell::{self, Shell};
use crate::cli::spec;

pub mod arg;
pub mod flag;
pub mod operand;

/// Keep track of an instance of an executable command. This differs from
/// spec::Command because this contains an actual list of parsed Flags
/// and Operands that have values.
#[derive(Debug)]
pub struct Command<'a, Context> {
    spec: &'a spec::Command<Context>,
    flags: flag::FlagSet<'a>,
    operands: operand::OperandList,
}

impl<'a, Context> Command<'a, Context> {
    pub fn new(
        spec: &'a spec::Command<Context>,
        flags: flag::FlagSet<'a>,
        operands: operand::OperandList
    ) -> Command<'a, Context> {
        Command { spec, flags, operands }
    }

    pub fn spec(&self) -> &spec::Command<Context> {
        &self.spec
    }

    pub fn execute(
        &self,
        shell: &Shell<Context>,
        state: &mut shell::State,
        context: &mut Context
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
}
