use crate::account::{Account, AccountPool};
use crate::errors::Error;
use crate::hasher::Hasher;
use std::collections::HashMap;
use std::convert::Into;
use std::sync::Arc;
/// 基于测试账户管理
#[derive(Debug)]
pub struct AccTestPool {
    pool: HashMap<String, Arc<Account>>,
}

impl AccTestPool {
    pub fn new() -> Box<dyn AccountPool> {
        let mut pool = AccTestPool {
            pool: HashMap::<String, Arc<Account>>::default(),
        };
        for _ in 0..3 {
            let acc = Account::new(1, 1, false, true).unwrap();
            let addr = acc.encode().unwrap();
            pool.pool.insert(addr, Arc::new(acc));
        }
        Box::new(pool)
    }
}

impl AccountPool for AccTestPool {
    fn get_account(&self, id: &str) -> Result<Arc<Account>, Error> {
        self.pool
            .get(id.into())
            .map_or(Error::msg("not found"), |v| Ok(v.clone()))
    }
    fn list_keys(&self) -> Vec<String> {
        let mut keys = vec![];
        for (key, _) in self.pool.iter() {
            keys.push(key.clone());
        }
        keys
    }
    fn len(&self) -> usize {
        self.pool.len()
    }
}

#[test]
fn test_acc_test_pool() {
    let p = AccTestPool::new();
    let keys = p.list_keys();
    assert_eq!(keys.len(), 3);
}
