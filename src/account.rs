use crate::bytes::{FromBytes, IntoBytes};
use crate::consts::{ACC_HRP, ADDR_HRP, MAX_ACCOUNT_KEY_SIZE};
use crate::crypto::{bech32_decode, PriKey, PubKey, SigValue};
use crate::errors;
use crate::hasher::{Hasher, SIZE as HasherSize};
use crate::iobuf;
use crate::iobuf::{Reader, Writer};
use crate::util;
use bech32::{FromBase32, ToBase32, Variant};
use hex::{FromHex, ToHex};

/// 账户管理器
pub trait AccountPool: Sync + Send {
    /// 获取指定的账户
    fn get_account(&self, id: &Hasher) -> Result<Account, errors::Error>;
    /// 列出所有账户
    fn list_account(&self) -> Vec<&Account>;
}

/// 存在地址hasher可获取地址
/// Account,TxIn,TxOut,Hasher
pub trait HasAddress: Sized {
    /// 获取地址hasher
    fn get_address(&self) -> Result<Hasher, errors::Error>;
    /// 获取地址hash并转为字符串
    fn string(&self) -> Result<String, errors::Error> {
        let h = self.get_address()?;
        Account::encode_with_hasher(ADDR_HRP, &h)
    }
}

/// 账户结构 多个私钥组成
/// 按顺序链接后hasher生成地址
#[derive(Debug, Clone)]
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

#[test]
fn test_encode_decode() {
    let acc = Account::new(2, 2, false, true).unwrap();
    let addr1 = acc.encode().unwrap();
    let hash = Account::decode(&addr1).unwrap();
    let addr2 = Account::encode_with_hasher(ADDR_HRP, &hash).unwrap();
    assert_eq!(addr1, addr2)
}

#[test]
fn test_account_hex_string() {
    let acc1 = Account::new(2, 2, false, true).unwrap();
    let acc2 = Account::decode_from_hex(&acc1.encode_to_hex().unwrap()).unwrap();
    assert_eq!(acc1, acc2);
}

#[test]
fn test_account_bech32_string() {
    let acc1 = Account::new(2, 2, false, true).unwrap();
    let acc2 = Account::decode_from_bech32(&acc1.encode_to_bech32().unwrap()).unwrap();
    assert_eq!(acc1, acc2);
}

impl HasAddress for Account {
    /// 从账号获取地址
    fn get_address(&self) -> Result<Hasher, errors::Error> {
        self.hash()
    }
}

impl Account {
    /// 编码为16进制字符串
    pub fn encode_to_hex(&self) -> Result<String, errors::Error> {
        let mut wb = Writer::default();
        self.encode_to_writer(&mut wb)?;
        Ok(wb.bytes().encode_hex())
    }
    /// 从16进制解码数据
    pub fn decode_from_hex(value: &str) -> Result<Self, errors::Error> {
        let buf: Result<Vec<u8>, hex::FromHexError> = Vec::from_hex(value.as_bytes());
        buf.map_or_else(errors::Error::std, |v| {
            let mut reader = Reader::new(&v);
            Self::decode_from_reader(&mut reader)
        })
    }
    /// 编码为bech32编码
    pub fn encode_to_bech32(&self) -> Result<String, errors::Error> {
        let mut wb = Writer::default();
        self.encode_to_writer(&mut wb)?;
        bech32::encode(ACC_HRP, wb.bytes().to_base32(), Variant::Bech32m)
            .map_or(errors::Error::msg("encode bech32 error"), |v| Ok(v))
    }
    /// 从bech32解码数据
    pub fn decode_from_bech32(value: &str) -> Result<Self, errors::Error> {
        let (hrp, buf) =
            bech32_decode(value).map_or(errors::Error::msg("decode bech32 error"), |v| Ok(v))?;
        if hrp != ACC_HRP {
            return errors::Error::msg("acc hrp error");
        }
        let mut reader = Reader::new(&buf);
        Self::decode_from_reader(&mut reader)
    }
    /// 序列化账户信息到写入器
    pub fn encode_to_writer(&self, wb: &mut Writer) -> Result<(), errors::Error> {
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
        //写入校验和
        let sum = Hasher::sum(wb.bytes());
        wb.put_bytes(sum.as_bytes());
        Ok(())
    }
    /// 解码账户数据
    pub fn decode_from_reader(rb: &mut Reader) -> Result<Self, errors::Error> {
        let rbb = rb.clone();
        let num = rb.u8()?;
        let less = rb.u8()?;
        let arb = rb.u8()?;
        let mut acc = Account::new(num, less, arb != 0xFF, false)?;
        for i in 0..rb.u8()? as usize {
            match rb.get::<PubKey>() {
                Ok(key) => acc.pubs[i] = Some(key),
                Err(_) => acc.pubs[i] = None,
            }
        }
        for i in 0..rb.u8()? as usize {
            match rb.get::<PriKey>() {
                Ok(key) => acc.pris[i] = Some(key),
                Err(_) => acc.pris[i] = None,
            }
        }
        //读取校验和
        let sum: Hasher = rb.decode()?;
        //计算原始校验数据
        let bb = &rbb.bytes();
        let bb = &bb[0..bb.len() - HasherSize];
        if Hasher::sum(bb) != sum {
            return errors::Error::msg("check sum error");
        }
        if !acc.check_with_pubs() {
            return errors::Error::msg("check with pubs error");
        }
        if !acc.check_pris_pubs() {
            return errors::Error::msg("check with pric and pubs error");
        }
        Ok(acc)
    }
    ///保存到文件
    pub fn save(&self, path: &str) -> Result<(), errors::Error> {
        let mut wb = Writer::default();
        self.encode_to_writer(&mut wb)?;
        util::write_file(path, || wb.bytes())
    }
    /// 从文件加载数据
    pub fn load(path: &str) -> Result<Self, errors::Error> {
        util::read_file(path, |buf| {
            let mut reader = Reader::new(&buf);
            Account::decode_from_reader(&mut reader)
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
            return errors::Error::msg("InvalidAccount");
        }
        if self.pubs_size() != self.num {
            return errors::Error::msg("InvalidPublicKey");
        }
        wb.u8(self.num);
        wb.u8(self.less);
        wb.u8(self.arb);
        wb.u8(self.pubs_size() as u8);
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
            return errors::Error::msg("InvalidAccount");
        }
        if self.pubs_size() != self.num {
            return errors::Error::msg("InvalidPublicKey");
        }
        let mut wb = iobuf::Writer::default();
        wb.u8(self.num);
        wb.u8(self.less);
        wb.u8(self.arb);
        wb.u8(self.pubs_size() as u8);
        for pb in self
            .pubs
            .iter()
            .filter(|&v| v.is_some())
            .map(|v| v.as_ref().unwrap())
        {
            //公钥hash作为账户地址的一部分
            wb.put_bytes(&pb.hash().into_bytes());
        }
        Ok(wb.hash())
    }
    ///带前缀编码地址
    pub fn encode_with_hasher(hrp: &str, hv: &Hasher) -> Result<String, errors::Error> {
        let bb = hv.as_bytes();
        match bech32::encode(hrp, bb.to_base32(), Variant::Bech32m) {
            Ok(addr) => return Ok(addr),
            Err(_) => return errors::Error::msg("InvalidAccount"),
        }
    }
    ///带前缀编码地址
    pub fn encode_with_hrp(&self, hrp: &str) -> Result<String, errors::Error> {
        let hv = self.hash()?;
        Self::encode_with_hasher(hrp, &hv)
    }
    ///解码地址并返回前缀
    pub fn decode_with_hrp(str: &str) -> Result<(String, Hasher), errors::Error> {
        let (hpr, dat, variant) = bech32::decode(str).map_or_else(errors::Error::std, |v| Ok(v))?;
        if variant != Variant::Bech32m {
            return errors::Error::msg("bech32 varinat error");
        }
        let buf = Vec::<u8>::from_base32(&dat).map_or_else(errors::Error::std, |v| Ok(v))?;
        Ok((hpr, Hasher::with_bytes(&buf)))
    }
    /// 解码地址并验证
    pub fn decode(str: &str) -> Result<Hasher, errors::Error> {
        let (hpr, id) = Self::decode_with_hrp(str)?;
        if hpr != ADDR_HRP {
            return errors::Error::msg("hpr not match");
        }
        Ok(id)
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
                    Ok(())
                }
                Err(_) => errors::Error::msg("SignatureErr"),
            },
            None => errors::Error::msg("InvalidPrivateKey"),
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
    assert_eq!(3 + 1 + 33 * 5, wb.bytes().len());
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
