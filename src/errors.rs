use std::string::String;

/// global errors
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Error(String);

impl Error {
    pub fn msg<T>(s: &str) -> Result<T, Self> {
        Err(Error(s.into()))
    }
}
