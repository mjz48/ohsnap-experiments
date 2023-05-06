use std::error::Error;

/// Argument that is passed to a Flag. This should be parallel to spec::Arg.
/// Maybe there is a better way to do this than splitting spec::Arg and
/// command::Arg?
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum Arg {
    #[default]
    None,
    Optional(Option<String>),
    Required(String),
}

impl Arg {
    pub fn get<T: From<String>>(&self) -> Result<Option<T>, Box<dyn Error>> {
        match self {
            Arg::Optional(val) => Ok(Some(T::from(val.clone().unwrap()))),
            Arg::Required(s) => Ok(Some(T::from(s.clone()))),
            _ => Ok(None),
        }
    }

    pub fn raw(&self) -> Option<String> {
        match self {
            Arg::Optional(val) => val.clone(),
            Arg::Required(s) => Some(s.to_owned()),
            _ => None,
        }
    }
}
