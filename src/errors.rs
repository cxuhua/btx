/// global errors
#[derive(Copy, PartialEq, Eq, Clone, Debug)]
pub enum Error {
    InvalidAccount,
    InvalidPublicKey,
}

impl Error {
    fn as_str(&self) -> &str {
        match *self {
            InvalidAccount => "InvalidAccount",
            InvalidPublicKey => "InvalidPublicKey",
        }
    }
}
