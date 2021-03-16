/// global errors
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Error(String);

impl Error {
    /// 创建错误信息
    pub fn msg<T>(s: &str) -> Result<T, Self> {
        Err(Error(s.into()))
    }
    /// 从标准错误创建错误
    pub fn std<T, E>(err: E) -> Result<T, Self>
    where
        E: std::error::Error,
    {
        Self::msg(&err.to_string())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "btx error: {}", self.0)
    }
}

impl std::error::Error for Error {}
