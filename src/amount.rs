///
pub static COIN: i64 = 1000000;
pub static MAX_MONEY: i64 = 21000000 * COIN;

pub fn is_valid_amount(v: &i64) -> bool {
    *v >= 0 && *v <= MAX_MONEY
}

#[test]
fn test_amount() {
    println!("{}",is_valid_amount(&121));
}