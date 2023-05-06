#[derive(Debug)]
pub struct UnknownCommandError(pub String);

impl std::fmt::Display for UnknownCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Error: unknown command {}", self.0)
    }
}

impl std::error::Error for UnknownCommandError {}
