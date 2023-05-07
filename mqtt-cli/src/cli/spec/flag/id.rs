use std::hash::{Hash, Hasher};

/// Use this to uniquely identify a spec::Flag or command::Flag
#[derive(Clone, Debug, Eq)]
pub struct Id {
    name: String,
    short: char,
}

impl Id {
    pub fn new(name: &str, short: char) -> Id {
        Id { name: name.to_owned(), short }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn short(&self) -> char {
        self.short
    }
}

impl Hash for Id {
    fn hash<H: Hasher>(&self, state: &mut H) {
        format!("{}:{}", self.name, self.short).hash(state);
    }
}

impl PartialEq for Id {
    fn eq(&self, other: &Self) -> bool {
        format!("{}:{}", self.name, self.short) == format!("{}:{}", other.name, other.short)
    }
}
