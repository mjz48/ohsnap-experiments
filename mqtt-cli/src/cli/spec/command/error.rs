use std::fmt::{Display, Formatter, Result};

#[derive(Debug)]
pub struct UnknownCommandError(pub String);

impl Display for UnknownCommandError {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "Error: unknown command {}", self.0)
    }
}

impl std::error::Error for UnknownCommandError {}
