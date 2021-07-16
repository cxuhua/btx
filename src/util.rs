use crate::consts;
use crate::errors::Error;
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use rand::RngCore;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

/// 写入数据到文件
pub fn write_file<'a, F>(path: &str, f: F) -> Result<(), Error>
where
    F: Fn() -> &'a [u8],
{
    let path = Path::new(path);
    if fs::metadata(&path).map_or(0, |v| v.len()) > 0 {
        return Error::msg("file exists");
    }
    //文件写入
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(&path)
        .map_or_else(Error::std, |ref mut fd| {
            let buf = f();
            fd.write_all(&buf).map_or_else(Error::std, |v| Ok(v))
        })
}

/// 读取文件所有数据
pub fn read_file<F, R>(path: &str, f: F) -> Result<R, Error>
where
    F: FnOnce(&mut Vec<u8>) -> Result<R, Error>,
{
    let path = Path::new(path);
    if fs::metadata(&path).map_or(0, |v| v.len()) <= 0 {
        return Error::msg("file length error");
    }
    fs::OpenOptions::new()
        .read(true)
        .open(&path)
        .map_or_else(Error::std, |ref mut fd| {
            let mut buf = Vec::new();
            fd.read_to_end(&mut buf)
                .map_or_else(Error::std, |_| f(&mut buf))
        })
}

/// 获取32位随机数
pub fn rand_u32() -> u32 {
    let mut rng = rand::thread_rng();
    rng.next_u32()
}

#[test]
fn test_rand_u32() {
    assert_ne!(rand_u32(), rand_u32());
}

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
