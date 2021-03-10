use crate::bytes::{FromBytes, IntoBytes};
use crate::consts::PK_HRP;
use crate::errors;
use crate::hasher::Hasher;
use core::{fmt, str};
use secp256k1::rand::rngs::OsRng;
use secp256k1::{Error, Message, PublicKey, Secp256k1, SecretKey, Signature};

//验证
fn verify(msg: &[u8], sig: &SigValue, pubkey: &PublicKey) -> Result<bool, Error> {
    let ctx = Secp256k1::verification_only();
    let uv = Hasher::hash(msg);
    let msg = Message::from_slice(uv.to_bytes())?;
    Ok(ctx.verify(&msg, &sig.inner, &pubkey).is_ok())
}

//签名
fn sign(msg: &[u8], seckey: &SecretKey) -> Result<Signature, Error> {
    let ctx = Secp256k1::signing_only();
    let uv = Hasher::hash(msg);
    let msg = Message::from_slice(uv.to_bytes())?;
    Ok(ctx.sign(&msg, seckey))
}

///签名结果
#[derive(Debug)]
pub struct SigValue {
    inner: Signature,
}

impl Clone for SigValue {
    fn clone(&self) -> Self {
        SigValue { inner: self.inner }
    }
}

impl str::FromStr for SigValue {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match Signature::from_str(s) {
            Ok(inner) => Ok(SigValue { inner: inner }),
            _ => Err(Error::InvalidSecretKey),
        }
    }
}

impl fmt::Display for SigValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::LowerHex::fmt(self, f)
    }
}

impl fmt::LowerHex for SigValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let sig = self.inner.serialize_der();
        for v in sig.iter() {
            write!(f, "{:02x}", v)?;
        }
        Ok(())
    }
}

impl FromBytes for SigValue {
    fn from_bytes(bb: &Vec<u8>) -> Result<Self, errors::Error> {
        let inner = Signature::from_der(&bb).unwrap();
        Ok(SigValue { inner: inner })
    }
}

impl IntoBytes for SigValue {
    fn into_bytes(&self) -> Vec<u8> {
        let sig = self.inner.serialize_der();
        sig.as_ref().to_vec()
    }
}

#[derive(Debug)]
pub struct PubKey {
    inner: PublicKey,
}

impl Clone for PubKey {
    fn clone(&self) -> Self {
        PubKey { inner: self.inner }
    }
}

impl str::FromStr for PubKey {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match PublicKey::from_str(s) {
            Ok(inner) => Ok(PubKey { inner: inner }),
            _ => Err(Error::InvalidSecretKey),
        }
    }
}

impl FromBytes for PubKey {
    fn from_bytes(bb: &Vec<u8>) -> Result<Self, errors::Error> {
        let inner = PublicKey::from_slice(&bb).unwrap();
        Ok(PubKey { inner: inner })
    }
}

impl IntoBytes for PubKey {
    fn into_bytes(&self) -> Vec<u8> {
        let sig = self.inner.serialize();
        sig.to_vec()
    }
}

impl fmt::Display for PubKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::LowerHex::fmt(self, f)
    }
}

impl fmt::LowerHex for PubKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.inner.fmt(f)
    }
}

use bech32::{FromBase32, ToBase32};

impl PubKey {
    ///验证签名数据
    pub fn verify(&self, msg: &[u8], sig: &SigValue) -> Result<bool, Error> {
        verify(msg, sig, &self.inner)
    }
    /// 编码公钥
    /// pk 开头的为公钥id
    pub fn encode(&self) -> Result<String, bech32::Error> {
        let v = self.hash();
        let b = v.to_bytes();
        bech32::encode(PK_HRP, b.to_base32())
    }
}

///解码bech32格式的字符串
pub fn bech32_decode(s: &str) -> Result<(String, Vec<u8>), bech32::Error> {
    let (hrp, data) = bech32::decode(&s)?;
    let vdata = Vec::<u8>::from_base32(&data)?;
    Ok((String::from(hrp), vdata))
}

#[test]
fn test_pubkyeid() {
    let kv = PriKey::new();
    let pv = kv.pubkey();
    let id = pv.encode().unwrap();
    println!("{}", id);
    println!("{:?}", bech32_decode(&id).unwrap());
}

impl PubKey {
    /// 将公钥再次hash用来生成账户地址
    pub fn hash(&self) -> Hasher {
        let v = self.inner.serialize();
        Hasher::hash(&v[..])
    }
}

#[derive(Debug)]
pub struct PriKey {
    inner: SecretKey,
}

impl Clone for PriKey {
    fn clone(&self) -> Self {
        PriKey { inner: self.inner }
    }
}

impl PriKey {
    ///签名指定的数据
    pub fn sign(&self, msg: &[u8]) -> Result<SigValue, Error> {
        match sign(msg, &self.inner) {
            Ok(sig) => Ok(SigValue { inner: sig }),
            Err(err) => Err(err),
        }
    }
    //推导对应的公钥
    pub fn pubkey(&self) -> PubKey {
        let ctx = Secp256k1::new();
        let inner = PublicKey::from_secret_key(&ctx, &self.inner);
        PubKey { inner: inner }
    }
}

impl FromBytes for PriKey {
    fn from_bytes(bb: &Vec<u8>) -> Result<Self, errors::Error> {
        let inner = SecretKey::from_slice(&bb).unwrap();
        Ok(PriKey { inner: inner })
    }
}

impl IntoBytes for PriKey {
    fn into_bytes(&self) -> Vec<u8> {
        self.inner[..].to_vec()
    }
}

impl str::FromStr for PriKey {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match SecretKey::from_str(s) {
            Ok(inner) => Ok(PriKey { inner: inner }),
            _ => Err(Error::InvalidSecretKey),
        }
    }
}

impl fmt::LowerHex for PriKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl fmt::Display for PriKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::LowerHex::fmt(self, f)
    }
}

impl Default for PriKey {
    fn default() -> Self {
        let mut rng = OsRng::new().unwrap();
        PriKey {
            inner: SecretKey::new(&mut rng),
        }
    }
}

impl PriKey {
    pub fn new() -> Self {
        PriKey::default()
    }
}

#[test]
fn test_iobuf() {
    use crate::iobuf::Writer;
    let mut wb = Writer::default();
    let pk = PriKey::new();
    wb.put(&pk);
    let mut rb = wb.reader();
    let _v2: PriKey = rb.get().unwrap();
}

#[test]
fn test_signer() {
    let kv = PriKey::new();
    let pv = kv.pubkey();
    let signature = kv.sign("adfs".as_bytes()).unwrap();
    println!("{:x?}", signature.into_bytes());
    println!("{}", signature);
    assert!(!pv.verify("adfs1".as_bytes(), &signature).unwrap());
    assert!(pv.verify("adfs".as_bytes(), &signature).unwrap());
}
