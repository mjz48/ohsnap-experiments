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
    pub fn get_as<T>(&self) -> Result<Option<T>, <T as std::str::FromStr>::Err>
        where T: std::str::FromStr + std::clone::Clone
    {
        match self {
            Arg::Optional(val) => {
                match val.clone().unwrap().parse() {
                    Ok(res) => { Ok(Some(res)) },
                    Err(error) => { Err(error) },
                }
            },
            Arg::Required(s) => {
                match s.clone().parse() {
                    Ok(res) => { Ok(Some(res)) },
                    Err(error) => { Err(error) },
                }
            },
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
