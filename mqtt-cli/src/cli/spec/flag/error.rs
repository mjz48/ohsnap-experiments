use std::error::Error;
use std::fmt::{Display, Formatter, Result};

use super::Query;

#[derive(Debug)]
pub struct UnknownFlagError(pub Query);

impl Display for UnknownFlagError {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "unrecognized flag '{}'", self.0)
    }
}

impl Error for UnknownFlagError {}

#[derive(Debug)]
pub struct FlagMissingArgError(pub Query);

impl Display for FlagMissingArgError {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "unrecognzied flag '{}'", self.0)
    }
}

impl Error for FlagMissingArgError {}

