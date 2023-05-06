use std::fmt;

use super::Flag;
use super::FlagSet;

/// Use this to search or compare flags based on text data
#[derive(Debug)]
pub enum Query {
    Name(String),
    Short(char),
}

impl fmt::Display for Query {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let flag_str = match self {
            Query::Name(s) => { format!("--{}", s) },
            Query::Short(c) => { format!("-{}", c) },
        };
        write!(f, "{}", flag_str)
    }
}

pub fn query_flag_spec<'a>(needle: &Query, haystack: &'a FlagSet) -> Option<&'a Flag> {
    for entry in haystack.iter() {
        if match needle {
            Query::Name(s) => *s == *entry.id.name(),
            Query::Short(c) => *c == entry.id.short(),
        } {
            return Some(&entry);
        }
    }
    return None;
}
