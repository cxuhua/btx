use crate::bytes::{Bytes, WithBytes};
use crate::u160::U160;
use crate::u256::U256;
use core::{fmt, str};
use secp256k1::rand::rngs::OsRng;
use secp256k1::{
    All, Error, Message, PublicKey, Secp256k1, SecretKey, SignOnly, Signature, Signing,
    Verification, VerifyOnly,
};

//验证
fn verify(msg: &[u8], sig: &SigValue, pubkey: &PublicKey) -> Result<bool, Error> {
    let ctx = Secp256k1::verification_only();
    let uv = U256::new(msg);
    let msg = Message::from_slice(uv.to_bytes())?;
    Ok(ctx.verify(&msg, &sig.inner, &pubkey).is_ok())
}

//签名
fn sign(msg: &[u8], seckey: &SecretKey) -> Result<Signature, Error> {
    let ctx = Secp256k1::signing_only();
    let uv = U256::new(msg);
    let msg = Message::from_slice(uv.to_bytes())?;
    Ok(ctx.sign(&msg, seckey))
}

///签名结果
pub struct SigValue {
    inner: Signature,
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

impl WithBytes<SigValue> for SigValue {
    fn with_bytes(bb: &Vec<u8>) -> SigValue {
        let inner = Signature::from_der(&bb).unwrap();
        SigValue { inner: inner }
    }
}

impl Bytes for SigValue {
    fn bytes(&self) -> Vec<u8> {
        let sig = self.inner.serialize_der();
        sig.as_ref().to_vec()
    }
}

pub struct PubKey {
    inner: PublicKey,
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

impl WithBytes<PubKey> for PubKey {
    fn with_bytes(bb: &Vec<u8>) -> PubKey {
        let inner = PublicKey::from_slice(&bb).unwrap();
        PubKey { inner: inner }
    }
}

impl Bytes for PubKey {
    fn bytes(&self) -> Vec<u8> {
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

impl PubKey {
    ///验证签名数据
    pub fn verify(&self, msg: &[u8], sig: &SigValue) -> Result<bool, Error> {
        verify(msg, sig, &self.inner)
    }
}

impl PubKey {
    pub fn hash(&self) -> U160 {
        let v = self.inner.serialize();
        U160::new(&v[..])
    }
}

pub struct PriKey {
    inner: SecretKey,
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

impl WithBytes<PriKey> for PriKey {
    fn with_bytes(bb: &Vec<u8>) -> PriKey {
        let inner = SecretKey::from_slice(&bb).unwrap();
        PriKey { inner: inner }
    }
}

impl Bytes for PriKey {
    fn bytes(&self) -> Vec<u8> {
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
    let v2: PriKey = rb.get();
}

#[test]
fn test_signer() {
    let kv = PriKey::new();
    let pv = kv.pubkey();
    let signature = kv.sign("adfs".as_bytes()).unwrap();
    println!("{:x?}", signature.bytes());
    println!("{}", signature);
    assert!(!pv.verify("adfs1".as_bytes(), &signature).unwrap());
    assert!(pv.verify("adfs".as_bytes(), &signature).unwrap());
}
