use std::time::SystemTime;

///获取当前时间戳
pub fn time_now() -> u32 {
    let now = SystemTime::now();
    match now.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(v) => v.as_secs() as u32,
        Err(_) => 0,
    }
}

#[test]
fn test_time_now() {
    println!("{}", time_now());
}
