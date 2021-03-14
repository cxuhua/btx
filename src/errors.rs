use std::string::String;

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
