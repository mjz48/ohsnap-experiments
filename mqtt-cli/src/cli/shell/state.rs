use std::collections::HashMap;
use std::sync::mpsc::{Sender};

#[derive(Clone, Debug)]
pub enum StateValue {
    String(String),
    Sender(Sender::<String>),
}

/// Store shell environment variables and state
pub type State = HashMap<String, StateValue>;
