use crate::bytes::{FromBytes, IntoBytes};
use crate::consts::{ADDR_HRP, MAX_ACCOUNT_KEY_SIZE};
use crate::crypto::{PriKey, PubKey, SigValue};
use crate::errors;
use crate::hasher::{Hasher, SIZE as HasherSize};
use crate::iobuf;
use crate::iobuf::{Reader, Writer};
use crate::util;
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
    //所有私钥,私钥只有在签名的时候才需要
    pris: Vec<Option<PriKey>>,
    //对应的公钥,必须存在的
    pubs: Vec<Option<PubKey>>,
    //存储的签名,有签名时存储,如果已经被对应的公钥签名时存在
    sigs: Vec<Option<SigValue>>,
}

/// 判断两个账户是否一致
impl PartialEq for Account {
    fn eq(&self, other: &Self) -> bool {
        self.num == other.num
            && self.less == other.less
            && self.arb == other.arb
            && self.pubs == other.pubs
    }
}

/// 转为脚本数据,不包含私钥
impl IntoBytes for Account {
    fn into_bytes(&self) -> Vec<u8> {
        let mut wb = Writer::default();
        wb.u8(self.num);
        wb.u8(self.less);
        wb.u8(self.arb);
        //pubs
        wb.u8(self.pubs_size());
        for iv in self.pubs.iter().filter(|&v| v.is_some()) {
            wb.put(iv.as_ref().unwrap());
        }
        //sigs
        wb.u8(self.sigs_size());
        for iv in self.sigs.iter().filter(|&v| v.is_some()) {
            wb.put(iv.as_ref().unwrap());
        }
        return wb.bytes().to_vec();
    }
}

/// 从脚本数据获取,不包含私钥
impl FromBytes for Account {
    fn from_bytes(bb: &Vec<u8>) -> Result<Account, errors::Error> {
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
        if !acc.check_with_pubs() {
            return errors::Error::msg("InvalidAccount");
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
    assert_eq!(true, acc.verify_with_public(0, "aaa".as_bytes()).unwrap());
    assert_eq!(true, acc.verify_with_public(1, "bbb".as_bytes()).unwrap());
}

#[test]
fn test_account_save_load() {
    use tempdir::TempDir;
    let tmp = TempDir::new("account").unwrap();
    let dir: String = tmp.path().to_str().unwrap().into();
    let dir = dir + "/acc.dat";
    let acc = Account::new(2, 2, false, true).unwrap();
    acc.save(&dir).unwrap();
    let old = Account::load(&dir).unwrap();
    assert_eq!(acc, old);
}

impl Account {
    ///保存到文件
    pub fn save(&self, path: &str) -> Result<(), errors::Error> {
        let mut wb = Writer::default();
        wb.u8(self.num);
        wb.u8(self.less);
        wb.u8(self.arb);
        wb.u8(self.pubs.len() as u8);
        for iv in self.pubs.iter() {
            match iv {
                Some(v) => {
                    wb.put(v);
                }
                None => {
                    wb.usize(0);
                }
            }
        }
        wb.u8(self.pris.len() as u8);
        for iv in self.pris.iter() {
            match iv {
                Some(v) => {
                    wb.put(v);
                }
                None => {
                    wb.usize(0);
                }
            }
        }
        //写入校验和数据到末尾
        let sum = Hasher::sum(wb.bytes());
        wb.put_bytes(sum.as_bytes());
        util::write_file(path, || wb.bytes())
    }
    /// 从文件加载数据
    pub fn load(path: &str) -> Result<Self, errors::Error> {
        util::read_file(path, |buf| {
            if buf.len() < HasherSize {
                return errors::Error::msg("buf too short");
            }
            let mut reader = Reader::new(&buf);
            let num = reader.u8()?;
            let less = reader.u8()?;
            let arb = reader.u8()?;
            //从文件加载数据
            let mut acc = Account::new(num, less, arb != 0xFF, false)?;
            for i in 0..reader.u8()? as usize {
                match reader.get::<PubKey>() {
                    Ok(key) => acc.pubs[i] = Some(key),
                    Err(_) => acc.pubs[i] = None,
                }
            }
            for i in 0..reader.u8()? as usize {
                match reader.get::<PriKey>() {
                    Ok(key) => acc.pris[i] = Some(key),
                    Err(_) => acc.pris[i] = None,
                }
            }
            //读取并检测校验和
            let sum1 = Hasher::with_bytes(&reader.get_bytes(HasherSize)?);
            let sum2 = Hasher::sum(&buf[0..buf.len() - HasherSize]);
            if sum1 != sum2 {
                return errors::Error::msg("check sum error");
            }
            if !acc.check_with_pubs() {
                return errors::Error::msg("check with pubs error");
            }
            if !acc.check_pris_pubs() {
                return errors::Error::msg("check with pric and pubs error");
            }
            Ok(acc)
        })
    }
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
        self.arb != 0xFF && self.arb == (self.num - 1)
    }
    ///创建一个默认账户
    ///is_gen 是否创建新的私钥
    pub fn new(num: u8, less: u8, arb: bool, is_gen: bool) -> Result<Account, errors::Error> {
        let mut acc = Account {
            num: num,
            less: less,
            arb: 0xFF,
            pris: vec![None; num as usize],
            pubs: vec![None; num as usize],
            sigs: vec![None; num as usize],
        };
        if is_gen {
            acc.initialize()
        }
        if arb {
            acc.arb = num - 1;
        }
        if !acc.check() {
            return errors::Error::msg("InvalidAccount");
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
    //检测私钥对应的公钥是否正确
    fn check_pris_pubs(&self) -> bool {
        if self.pubs_size() != self.pris_size() {
            return false;
        }
        for i in 0..self.pris.len() {
            let eq = match (&self.pubs[i], &self.pris[i]) {
                (None, None) => true,
                (Some(ref pb), Some(ref pk)) => &pk.pubkey() == pb,
                _ => false,
            };
            if !eq {
                return false;
            }
        }
        return true;
    }
    //从脚本获取时检测公钥和签名
    fn check_with_pubs(&self) -> bool {
        if !self.check() {
            return false;
        }
        //公钥数量检测
        if self.pubs_size() != self.num {
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
    /// 不带签名和私钥的数据
    pub fn encode_sign(&self, wb: &mut Writer) -> Result<(), errors::Error> {
        if !self.check() {
            return errors::Error::msg("InvalidPublicKey");
        }
        if self.pubs_size() != self.num {
            return errors::Error::msg("InvalidPublicKey");
        }
        wb.u8(self.num);
        wb.u8(self.less);
        wb.u8(self.arb);
        for pb in self
            .pubs
            .iter()
            .filter(|v| v.is_some())
            .map(|v| v.as_ref().unwrap())
        {
            wb.put_bytes(&pb.into_bytes());
        }
        Ok(())
    }
    //hash账户用于生成地址
    pub fn hash(&self) -> Result<Hasher, errors::Error> {
        if !self.check() {
            return errors::Error::msg("InvalidPublicKey");
        }
        if self.pubs_size() != self.num {
            return errors::Error::msg("InvalidPublicKey");
        }
        let mut wb = iobuf::Writer::default();
        wb.u8(self.num);
        wb.u8(self.less);
        wb.u8(self.arb);
        for pb in self
            .pubs
            .iter()
            .filter(|&v| v.is_some())
            .map(|v| v.as_ref().unwrap())
        {
            wb.put_bytes(&pb.hash().into_bytes());
        }
        let bb = wb.bytes();
        Ok(Hasher::hash(bb))
    }
    ///带前缀编码地址
    pub fn encode_with_hrp(&self, hrp: &str) -> Result<String, errors::Error> {
        let hv = self.hash()?;
        let bb = hv.as_bytes();
        match bech32::encode(hrp, bb.to_base32()) {
            Ok(addr) => return Ok(addr),
            Err(_) => return errors::Error::msg("InvalidAccount"),
        }
    }
    ///带固定前缀编码地址
    pub fn encode(&self) -> Result<String, errors::Error> {
        self.encode_with_hrp(ADDR_HRP)
    }
    ///使用指定的私钥签名消息 0 -> pris.len()
    pub fn sign_with_index(&mut self, idx: usize, msg: &[u8]) -> Result<(), errors::Error> {
        if idx >= self.pris.len() {
            return errors::Error::msg("InvalidParam");
        }
        match self.pris[idx] {
            Some(ref pk) => match pk.sign(msg) {
                Ok(sig) => {
                    self.sigs[idx] = Some(sig);
                    return Ok(());
                }
                Err(_) => {
                    return errors::Error::msg("SignatureErr");
                }
            },
            None => {
                return errors::Error::msg("InvalidPrivateKey");
            }
        }
    }
    ///使用指定的公钥验签所有签名,如果其中一个通过返回true
    pub fn verify_with_public(&self, ipub: usize, msg: &[u8]) -> Result<bool, errors::Error> {
        if ipub >= self.pubs.len() {
            return errors::Error::msg("InvalidParam");
        }
        //
        let pb = self.pubs[ipub].as_ref();
        if pb.is_none() {
            return errors::Error::msg("InvalidPublicKey");
        }
        let pb = pb.unwrap();
        //逐个验证签名
        for iv in self.sigs.iter().filter(|&v| v.is_some()) {
            let sb = iv.as_ref().unwrap();
            let ret = pb.verify(msg, sb);
            if ret.is_err() {
                continue;
            }
            if ret.unwrap() {
                return Ok(true);
            }
        }
        return errors::Error::msg("VerifySignErr");
    }
    /// 使用指定的公钥和签名验签所有签名,如果其中一个通过返回true
    fn verify_with_index(
        &self,
        ipub: usize,
        isig: usize,
        msg: &[u8],
    ) -> Result<bool, errors::Error> {
        if ipub >= self.pubs.len() {
            return errors::Error::msg("InvalidParam");
        }
        if isig >= self.sigs.len() {
            return errors::Error::msg("InvalidParam");
        }
        match (&self.pubs[ipub], &self.sigs[isig]) {
            (Some(pb), Some(sb)) => match pb.verify(msg, sb) {
                Ok(rb) => Ok(rb),
                _ => errors::Error::msg("VerifySignErr"),
            },
            _ => return errors::Error::msg("InvalidParam"),
        }
    }
    /// 标准验签
    /// msg数据为签名数据,不需要进行hash,签名时会进行一次Hasher::hash*
    pub fn verify(&self, msg: &[u8]) -> Result<bool, errors::Error> {
        //检测账户是否包含公钥
        if !self.check_with_pubs() {
            return errors::Error::msg("InvalidAccount");
        }
        //不启用仲裁时最小签名数量
        if !self.use_arb() && self.sigs_size() < self.less {
            return errors::Error::msg("InvalidAccount");
        }
        //启用时至少一个签名
        if self.use_arb() && self.sigs_size() < 1 {
            return errors::Error::msg("InvalidAccount");
        }
        //验证仲裁公钥签名
        if self.use_arb() {
            return self.verify_with_public(self.arb as usize, msg);
        }
        //检测是否达到签名要求
        let mut less = self.less;
        let (mut i, mut j) = (0, 0);
        while i < self.sigs.len() && j < self.pubs.len() {
            let sig = &self.sigs[i];
            if sig.is_none() {
                i += 1; //下个签名
                continue;
            }
            let rb = self.verify_with_index(j, i, msg);
            if rb.is_ok() && rb.unwrap() {
                less -= 1;
                i += 1; //下个签名
            }
            if less == 0 {
                break;
            }
            j += 1; //下个公钥
        }
        Ok(less == 0)
    }
}

#[test]
fn test_account_encode_sign() {
    let wb = &mut Writer::default();
    let acc = Account::new(5, 2, false, true).unwrap();
    acc.encode_sign(wb).unwrap();
    assert_eq!(3 + 33 * 5, wb.bytes().len());
}

#[test]
fn test_account_verify_true() {
    let mut acc = Account::new(5, 2, false, true).unwrap();
    acc.sign_with_index(0, "aaa".as_bytes()).unwrap();
    acc.sign_with_index(1, "aaa".as_bytes()).unwrap();
    assert_eq!(true, acc.verify("aaa".as_bytes()).unwrap());
}

#[test]
fn test_account_verify_false() {
    let mut acc = Account::new(5, 2, false, true).unwrap();
    acc.sign_with_index(0, "aaa".as_bytes()).unwrap();
    acc.sign_with_index(1, "bbb".as_bytes()).unwrap();
    assert_eq!(false, acc.verify("aaa".as_bytes()).unwrap());
}
