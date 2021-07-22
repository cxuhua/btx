use crate::account::{Account, AccountPool};
use crate::errors::Error;
use crate::hasher::Hasher;
use crate::index::IKey;
use std::collections::HashMap;
use std::convert::Into;
/// 基于测试账户管理
#[derive(Debug)]
pub struct AccTestPool {
    pool: HashMap<IKey, Account>,
}

impl AccTestPool {
    pub fn new() -> Box<dyn AccountPool> {
        let mut pool = AccTestPool {
            pool: HashMap::<IKey, Account>::default(),
        };
        for _ in 0..3 {
            let acc = Account::new(1, 1, false, true).unwrap();
            let id = acc.hash().unwrap();
            pool.pool.insert(id.as_ref().into(), acc);
        }
        Box::new(pool)
    }
}

impl AccountPool for AccTestPool {
    fn get_account(&self, id: &Hasher) -> Result<Account, Error> {
        self.pool
            .get(&id.into())
            .map_or(Error::msg("not found"), |v| Ok(v.clone()))
    }
    fn list_keys(&self) -> Vec<Hasher> {
        let mut keys: Vec<Hasher> = vec![];
        for (key, _) in self.pool.iter() {
            keys.push(key.bytes().into());
        }
        keys
    }
}

#[test]
fn test_acc_test_pool() {
    let p = AccTestPool::new();
    let keys = p.list_keys();
    assert_eq!(keys.len(), 3);
}
