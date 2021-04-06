use crate::consts;
use crate::errors::Error;
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use std::fs;
use std::path::Path;

/// 如果目录丢失自动创建目录
pub fn miss_create_dir(dir: &str) -> Result<(), Error> {
    let p = Path::new(dir);
    //目录是否存在
    if fs::metadata(p).map_or(false, |v| v.is_dir()) {
        return Ok(());
    }
    //创建目录
    fs::create_dir(&p).map_or_else(Error::std, |_| Ok(()))
}

/// 获取当前时间戳
/// 2020-01-01 00:00:00 UTC 开始至今的秒数
pub fn timestamp() -> i64 {
    Utc::now().timestamp() - consts::BASE_UTC_UNIX_TIME
}

/// 从时间戳获取系统时间
pub fn from_timestamp(now: i64) -> DateTime<Utc> {
    let unix = now + consts::BASE_UTC_UNIX_TIME;
    let ndt = NaiveDateTime::from_timestamp(unix, 0);
    DateTime::from_utc(ndt, Utc)
}

/// 从年月日时分秒获取
pub fn from_ymd_hms(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> DateTime<Utc> {
    let d = NaiveDate::from_ymd(y, mo, d).and_hms(h, mi, s);
    DateTime::from_utc(d, Utc)
}

#[test]
fn test_time_now() {
    let y = from_ymd_hms(2020, 01, 01, 00, 00, 00);
    let x = from_timestamp(0);
    assert_eq!(x, y);

    let y = from_ymd_hms(2020, 01, 01, 00, 00, 30);
    let x = from_timestamp(30);
    assert_eq!(x, y);
}

#[test]
fn test_log_use() {
    struct Logger;
    impl log::Log for Logger {
        fn enabled(&self, _: &log::Metadata) -> bool {
            false
        }
        fn log(&self, record: &log::Record) {
            println!("{:?}", record);
        }
        fn flush(&self) {}
    }
    log::set_boxed_logger(Box::new(Logger)).unwrap();
    log::set_max_level(log::LevelFilter::Trace);
    log::info!("aaa{},{}", 111, 222);
}
