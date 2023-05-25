use std::collections::HashSet;
use std::hash::{Hash, Hasher};

use super::arg::Arg;
pub use id::*;
pub use query::*;

pub mod error;
mod id;
mod query;

pub type FlagSet = HashSet<Flag>;

/// Specification for command line flag
#[derive(Clone, Debug, Eq)]
pub struct Flag {
    id: id::Id,
    help: String,
    arg: Arg,
}

impl Flag {
    pub fn new(name: &str, short: char, arg: Arg, help: &str) -> Flag {
        let id = id::Id::new(name, short);
        Flag {
            id,
            arg,
            help: help.to_owned(),
        }
    }

    pub fn id(&self) -> &Id {
        &self.id
    }

    pub fn arg_spec(&self) -> &Arg {
        &self.arg
    }

    pub fn help(&self) -> &str {
        &self.help
    }
}

impl Hash for Flag {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl PartialEq for Flag {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl AsRef<str> for Flag {
    fn as_ref(&self) -> &str {
        self.id().name()
    }
}

/// check if a string is a long flag
pub fn is_long(flag_text: &str) -> bool {
    flag_text.starts_with("--") && flag_text.len() > 3
}

/// check if a string is a short flag
pub fn is_short(flag_text: &str) -> bool {
    flag_text.starts_with("-") && flag_text.len() > 1
}

/// check if a string is a flag
pub fn is_flag(flag_text: &str) -> bool {
    is_long(&flag_text) || is_short(&flag_text)
}

/// convert text string to flag query; if text is not a flag, return None
pub fn extract(flag_text: &str) -> Option<Query> {
    if is_long(&flag_text) {
        Some(Query::Name(
            flag_text.strip_prefix("--").unwrap().to_string(),
        ))
    } else if is_short(&flag_text) {
        // short flags are complicated
        // you can have the follwing forms:
        // 1) -a [optarg] e.g. -a myarg
        // 2) -a[optarg] e.g. -amyarg
        // 3) -abc e.g. -a -b -c
        Some(Query::Short(flag_text.chars().nth(1).unwrap()))
    } else {
        None
    }
}

/// Find a flag in a FlagSet
pub fn query<'a>(needle: &Query, haystack: &'a FlagSet) -> Option<&'a Flag> {
    for entry in haystack.iter() {
        if match needle {
            Query::Name(ref s) => *s == entry.id().name(),
            Query::Short(ref c) => *c == entry.id().short(),
        } {
            return Some(&entry);
        }
    }
    return None;
}
