use crate::account::{Account, AccountPool};
use crate::errors::Error;
use std::collections::HashMap;
use std::sync::Arc;
/// 基于测试账户管理
#[derive(Debug)]
pub struct AccTestPool {
    pool: HashMap<String, Arc<Account>>,
    keys: Vec<String>,
}

impl AccTestPool {
    pub fn new() -> Box<dyn AccountPool> {
        let mut pool = AccTestPool {
            pool: HashMap::<String, Arc<Account>>::default(),
            keys: vec![],
        };
        for _ in 0..3 {
            let acc = Account::new(1, 1, false, true).unwrap();
            let addr = acc.encode().unwrap();
            pool.keys.push(addr.clone());
            pool.pool.insert(addr, Arc::new(acc));
        }
        Box::new(pool)
    }
}

impl AccountPool for AccTestPool {
    fn get_account(&self, id: &str) -> Result<Arc<Account>, Error> {
        self.pool
            .get(id)
            .map_or(Error::msg("not found"), |v| Ok(v.clone()))
    }
    fn list_keys(&self) -> &Vec<String> {
        &self.keys
    }
    fn len(&self) -> usize {
        self.pool.len()
    }
    fn index(&self, idx: usize) -> Result<Arc<Account>, Error> {
        if idx >= self.len() {
            return Error::msg("idx error");
        }
        self.get_account(&self.keys[idx])
    }
}

#[test]
fn test_acc_test_pool() {
    let p = AccTestPool::new();
    let keys = p.list_keys();
    assert_eq!(keys.len(), 3);
}
