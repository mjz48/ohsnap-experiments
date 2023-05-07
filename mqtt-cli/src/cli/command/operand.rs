pub type OperandList = Vec<Operand>;

pub mod error;

/// An argument passed to a command to be operated upon
#[derive(Clone, Debug)]
pub struct Operand {
    value: String,
}

impl Operand {
    pub fn new(value: &str) -> Operand {
        Operand { value: value.into() }
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn get_as<T>(&self) -> Result<T, <T as std::str::FromStr>::Err>
        where T: std::str::FromStr + std::clone::Clone
    {
        self.value.clone().parse()
    }
}
