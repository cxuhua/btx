/// global errors
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Error(String);
use std::fmt;

impl Error {
    /// 创建错误
    pub fn error(s: &str) -> Self {
        Error(s.into())
    }
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
    /// 格式化
    pub fn fmt<T>(args: fmt::Arguments<'_>) -> Result<T, Self> {
        let str = fmt::format(args);
        Err(Error(str))
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "btx error: {}", self.0)
    }
}

impl std::error::Error for Error {}

#[test]
fn test_error_fmt() {
    fn ff() -> Result<(), Error> {
        Error::fmt(format_args!("{} foo {:?}", 1, 2))
    }
    assert_eq!(ff(), Err(Error("1 foo 2".into())));

    env_logger::builder()
        .filter(Some("btx"), log::LevelFilter::Trace)
        .init();
    log::trace!("--trace");
    log::debug!("--debug");
    log::info!("--info");
    log::warn!("--warn");
    log::error!("--error");
}
