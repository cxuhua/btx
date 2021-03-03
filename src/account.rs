use crate::bytes::{Bytes, WithBytes};
use crate::consts::{ADDR_HRP, MAX_ACCOUNT_KEY_SIZE};
use crate::crypto::{PriKey, PubKey, SigValue};
use crate::errors;
use crate::hasher::Hasher;
use crate::iobuf;
use crate::iobuf::{Reader, Writer};
use bech32::ToBase32;
/// 账户结构 多个私钥组成
/// 按顺序链接后hasher生成地址
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

///转为脚本数据,不包含私钥
impl Bytes for Account {
    fn bytes(&self) -> Vec<u8> {
        let mut wb = Writer::default();
        wb.u8(self.num);
        wb.u8(self.less);
        wb.u8(self.arb);
        //pubs
        wb.u8(self.pubs_size());
        for iv in self.pubs.iter() {
            if let Some(pb) = iv {
                wb.put(pb);
            }
        }
        //sigs
        wb.u8(self.sigs_size());
        for iv in self.sigs.iter() {
            if let Some(sb) = iv {
                wb.put(sb);
            }
        }
        return wb.bytes().to_vec();
    }
}

//从脚本数据获取,不包含私钥
impl WithBytes for Account {
    fn with_bytes(bb: &Vec<u8>) -> Result<Account, errors::Error> {
        let mut r = Reader::new(bb);
        let num = r.u8()?;
        let less = r.u8()?;
        let arb = r.u8()?;
        let mut acc = Account::new(num, less, arb != 0xFF, false)?;
        for i in 0..r.u8()? as usize {
            acc.pubs[i] = Some(r.get()?);
        }
        for i in 0..r.u8()? as usize {
            acc.sigs[i] = Some(r.get()?);
        }
        if !acc.check_with_script() {
            return Err(errors::Error::InvalidAccount);
        }
        Ok(acc)
    }
}

#[test]
fn test_account() {
    let mut acc = Account::new(2, 2, false, true).unwrap();
    acc.sign_with_index(0, "aaa".as_bytes()).unwrap();
    acc.sign_with_index(1, "bbb".as_bytes()).unwrap();
    let mut wb = Writer::default();
    wb.put(&acc);
    let mut rb = wb.reader();
    let tmp: Account = rb.get().unwrap();
    assert_eq!(acc.num, tmp.num);
    assert_eq!(acc.less, tmp.less);
    assert_eq!(acc.arb, tmp.arb);
    assert_eq!(2, tmp.pubs.len());
    assert_eq!(2, tmp.sigs.len());
    assert_eq!(true, acc.verify_with_index(0, "aaa".as_bytes()).unwrap());
    assert_eq!(true, acc.verify_with_index(1, "bbb".as_bytes()).unwrap());
}

impl Account {
    ///有效公钥数量
    pub fn pubs_size(&self) -> u8 {
        self.pubs.iter().filter(|&v| v.is_some()).count() as u8
    }
    ///有效签名数量
    pub fn sigs_size(&self) -> u8 {
        self.sigs.iter().filter(|&v| v.is_some()).count() as u8
    }
    ///有效私钥数量
    pub fn pris_size(&self) -> u8 {
        self.pris.iter().filter(|&v| v.is_some()).count() as u8
    }
    ///是否启用了仲裁公钥
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
            acc.sigs.push(None);
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
    //从脚本获取时检测
    fn check_with_script(&self) -> bool {
        if !self.check() {
            return false;
        }
        //公钥数量检测
        if self.pubs_size() != self.num {
            return false;
        }
        //不启用仲裁时最小签名数量
        if !self.use_arb() && self.sigs_size() != self.less {
            return false;
        }
        true
    }
    ///检测是否有效
    fn check(&self) -> bool {
        //最少一个 最多16个公钥
        if self.num < 1 || self.num > MAX_ACCOUNT_KEY_SIZE {
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
        true
    }
    //hash账户用于生成地址
    pub fn hash(&self) -> Result<Hasher, errors::Error> {
        if !self.check() {
            return Err(errors::Error::InvalidPublicKey);
        }
        let mut wb = iobuf::Writer::default();
        wb.u8(self.num);
        wb.u8(self.less);
        wb.u8(self.arb);
        if self.pubs.len() != self.num as usize {
            return Err(errors::Error::InvalidPublicKey);
        }
        for pb in self.pubs.iter() {
            match pb {
                Some(pb) => {
                    //公钥的hash值作为地址生成的一部分
                    wb.put(&pb.hash());
                }
                None => {
                    return Err(errors::Error::InvalidPublicKey);
                }
            }
        }
        let bb = wb.bytes();
        Ok(Hasher::hash(bb))
    }
    ///带前缀编码地址
    pub fn encode_with_hrp(&self, hrp: &str) -> Result<String, errors::Error> {
        let hv = self.hash()?;
        let bb = hv.to_bytes();
        match bech32::encode(hrp, bb.to_base32()) {
            Ok(addr) => return Ok(addr),
            Err(_) => return Err(errors::Error::InvalidAccount),
        }
    }
    ///带固定前缀编码地址
    pub fn encode(&self) -> Result<String, errors::Error> {
        self.encode_with_hrp(ADDR_HRP)
    }
    ///使用指定的私钥签名消息 0 -> pris.len()
    pub fn sign_with_index(&mut self, idx: usize, msg: &[u8]) -> Result<(), errors::Error> {
        if idx >= self.pris.len() {
            return Err(errors::Error::InvalidParam);
        }
        match &self.pris[idx] {
            Some(pk) => match pk.sign(msg) {
                Ok(sig) => {
                    self.sigs[idx] = Some(sig);
                    return Ok(());
                }
                Err(_) => {
                    return Err(errors::Error::SignatureErr);
                }
            },
            None => {
                return Err(errors::Error::InvalidPrivateKey);
            }
        }
    }
    ///使用指定的公钥和签名验签
    pub fn verify_with_index(&self, idx: usize, msg: &[u8]) -> Result<bool, errors::Error> {
        if idx >= self.pubs.len() {
            return Err(errors::Error::InvalidParam);
        }
        if idx >= self.sigs.len() {
            return Err(errors::Error::InvalidParam);
        }
        match &self.pubs[idx] {
            Some(pb) => match &self.sigs[idx] {
                Some(sig) => match pb.verify(msg, &sig) {
                    Ok(vb) => return Ok(vb),
                    Err(_) => return Err(errors::Error::VerifySignErr),
                },
                None => {
                    return Err(errors::Error::InvalidSignature);
                }
            },
            None => {
                return Err(errors::Error::InvalidPublicKey);
            }
        }
    }
}
