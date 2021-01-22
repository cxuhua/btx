///公钥前缀
pub const PK_HRP: &str = "pk";
///地址前缀
pub const ADDR_HRP: &str = "btx";
///脚本最大长度
pub const MAX_SCRIPT_SIZE: usize = 4096;
///脚本最大ops数量
pub const MAX_SCRIPT_OPS: usize = 256;
///一个coin的缩放比例
pub const COIN: i64 = 1000000;
///最大金额
pub const MAX_MONEY: i64 = 21000000 * COIN;
///账户最大密钥数
pub const MAX_ACCOUNT_KEY_SIZE:u8 = 16;
///检测金额是否在正常的范围内
pub fn is_valid_amount(v: &i64) -> bool {
    *v >= 0 && *v <= MAX_MONEY
}