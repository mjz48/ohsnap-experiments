use std::cmp::{Eq, PartialEq};
use std::collections::HashSet;
use std::hash::{Hash, Hasher};

use crate::spec;
use crate::spec::flag;
use super::arg::Arg;

pub type FlagSet<'a> = HashSet<Flag<'a>>;

/// A flag is a specific instance of a command line flag containing an actual
/// argument value
#[derive(Clone, Debug, Eq)]
pub struct Flag<'a> {
    spec: &'a spec::Flag,
    arg: Arg,
}

impl<'a> Flag<'a> {
    pub fn new(spec: &spec::Flag, arg: Arg) -> Flag {
        Flag { spec, arg }
    }

    pub fn spec(&self) -> &'a spec::Flag {
        self.spec
    }

    pub fn arg(&self) -> &Arg {
        &self.arg
    }
}

impl<'a> Hash for Flag<'a> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.spec.id().hash(state);
    }
}

impl<'a> PartialEq for Flag<'a> {
    fn eq(&self, other: &Self) -> bool {
        *self.spec.id() == *other.spec.id()
    }
}

/// Find a flag in a FlagSet
pub fn query<'a>(needle: &flag::Query, haystack: &'a FlagSet) -> Option<&'a Flag<'a>> {
    for entry in haystack.iter() {
        if match needle {
            flag::Query::Name(ref s) => *s == entry.spec().id().name(),
            flag::Query::Short(ref c) => *c == entry.spec().id().short(),
        } {
            return Some(&entry);
        }
    }
    return None;
}
