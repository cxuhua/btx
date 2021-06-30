/// 公钥前缀
pub const PK_HRP: &str = "pk";

/// 账户前缀
pub const ACC_HRP: &str = "aps";

/// 地址前缀
pub const ADDR_HRP: &str = "btx";

/// 一个coin的缩放比例
pub const COIN: i64 = 1000000;

/// 获取coin
pub fn coin(c: usize) -> i64 {
    c as i64 * COIN
}
/// 最大金额
pub const MAX_MONEY: i64 = 21000000 * COIN;

/// 账户最大密钥数
pub const MAX_ACCOUNT_KEY_SIZE: u8 = 16;

/// 检测金额是否在正常的范围内
pub fn is_valid_amount(v: i64) -> bool {
    v >= 0 && v <= MAX_MONEY
}
/// 时间戳开始时间 2020-01-01 00:00:00
pub const BASE_UTC_UNIX_TIME: i64 = 1577836800;

/// 区块中最大交易数量
pub const MAX_TX_COUNT: u16 = 0xFFFF;
