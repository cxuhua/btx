use crate::crypto::{PriKey, PubKey, SigValue};
use crate::errors;
use core::fmt;

/// 账户结构 多个私钥组成
/// 经过按顺序链接后hash160生成地址
#[derive(Debug)]
pub struct Account {
    //公钥数量
    num: u8,
    //至少需要通过的签名数量
    less: u8,
    //仲裁公钥索引 =0xFF表示不启用
    arb: u8,
    //所有私钥
    pris: Vec<Option<PriKey>>,
    //对应的公钥
    pubs: Vec<Option<PubKey>>,
    //存储的签名,有签名时存储
    sigs: Vec<Option<SigValue>>,
}

impl Account {
    pub fn use_arb(&self) -> bool {
        self.arb != 0xFF
    }
    ///创建一个默认账户
    ///is_gen 是否创建新的私钥
    pub fn new(num: u8, less: u8, arb: bool, is_gen: bool) -> Result<Account, errors::Error> {
        let mut acc = Account {
            num: num,
            less: less,
            arb: 0xFF,
            pris: Vec::with_capacity(num as usize),
            pubs: Vec::with_capacity(num as usize),
            sigs: Vec::with_capacity(num as usize),
        };
        for _ in 0..acc.num {
            acc.pris.push(None);
            acc.pubs.push(None);
        }
        if is_gen {
            acc.initialize()
        }
        if arb {
            acc.arb = num - 1;
        }
        if !acc.check() {
            return Err(errors::Error::InvalidAccount);
        }
        Ok(acc)
    }
    ///创建新账户
    ///重新生成所有私钥和公钥
    pub fn initialize(&mut self) {
        for i in 0..(self.num as usize) {
            let pk = PriKey::new();
            let pb = pk.pubkey();
            self.pris[i] = Some(pk);
            self.pubs[i] = Some(pb);
        }
    }
    ///检测是否有效
    fn check(&self) -> bool {
        //最少一个 最多16个公钥
        if self.num < 1 || self.num > 16 {
            return false;
        }
        //至少不能超过公钥数量
        if self.less < 1 || self.less > self.num {
            return false;
        }
        //如果启用arb,必须至少两个公钥
        if self.use_arb() && self.num < 2 {
            return false;
        }
        //如果启用仲裁,仲裁应该是最后一个密钥
        if self.use_arb() && self.arb != self.num - 1 {
            return false;
        }
        //私钥数量错误
        if self.pris.len() != self.num as usize {
            return false;
        }
        //私钥数量应该等于公钥数量
        if self.pris.len() != self.pubs.len() {
            return false;
        }
        true
    }
    ///获取地址id
    pub fn id(&self) -> Result<String, errors::Error> {
        Err(errors::Error::InvalidPublicKey)
    }
}

#[test]
fn test_account() {
    let mut acc = Account::new(1, 1, false, true).unwrap();
    println!("{:#?}", acc);
}
