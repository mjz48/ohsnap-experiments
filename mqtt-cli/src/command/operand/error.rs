use std::error::Error;
use std::fmt::{Display, Formatter, Result};

use super::OperandList;

#[derive(Debug)]
pub struct MissingOperandError(pub OperandList, pub usize);

impl Display for MissingOperandError {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(
            f,
            "not enough operands provided. Received {:?}, expected {}",
            self.0.len(),
            self.1,
        )
    }
}

impl Error for MissingOperandError {}
