use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};

///2020-01-01 00:00:00的时间戳,所有时间戳从这个点开始算
const BASE_UTC_UNIX_TIME: i64 = 1577836800;

/// 获取当前时间戳
/// 2020-01-01 00:00:00 UTC 开始至今的秒数
pub fn timestamp() -> u32 {
    (Utc::now().timestamp() - BASE_UTC_UNIX_TIME) as u32
}

/// 从时间戳获取系统时间
pub fn from_timestamp(now: u32) -> DateTime<Utc> {
    let unix = now as i64 + BASE_UTC_UNIX_TIME;
    let ndt = NaiveDateTime::from_timestamp(unix as i64, 0);
    DateTime::from_utc(ndt, Utc)
}

///从年月日时分秒获取
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
