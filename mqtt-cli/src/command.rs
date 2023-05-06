use std::error::Error;

pub use arg::Arg;
use crate::shell::{Context, Shell};
use crate::spec;

pub mod arg;
pub mod flag;
pub mod operand;

/// Keep track of an instance of an executable command. This differs from
/// spec::Command because this contains an actual list of parsed Flags
/// and Operands that have values.
#[derive(Debug)]
pub struct Command<'a> {
    spec: &'a spec::Command,
    flags: flag::FlagSet<'a>,
    operands: operand::OperandList,
}

impl<'a> Command<'a> {
    pub fn new(
        spec: &'a spec::Command,
        flags: flag::FlagSet<'a>,
        operands: operand::OperandList
    ) -> Command<'a> {
        Command { spec, flags, operands }
    }

    pub fn spec(&self) -> &spec::Command {
        &self.spec
    }

    pub fn execute(&self, shell: &Shell, context: &mut Context)
        -> Result<spec::ReturnCode, Box<dyn Error>> {
        (self.spec.callback())(&self, shell, context)
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
