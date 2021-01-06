use crate::u160::U160;
use crate::u256::U256;
use core::{fmt, str};
use secp256k1::rand::rngs::OsRng;
use secp256k1::{
    Error, Message, PublicKey, Secp256k1, SecretKey, Signature, Signing, Verification,
};

//验证
fn verify<C: Verification>(
    secp: &Secp256k1<C>,
    msg: &[u8],
    sig: &SigValue,
    pubkey: &PublicKey,
) -> Result<bool, Error> {
    let uv = U256::new(msg);
    let msg = Message::from_slice(uv.bytes())?;
    Ok(secp.verify(&msg, &sig.inner, &pubkey).is_ok())
}

//签名
fn sign<C: Signing>(
    secp: &Secp256k1<C>,
    msg: &[u8],
    seckey: &SecretKey,
) -> Result<Signature, Error> {
    let uv = U256::new(msg);
    let msg = Message::from_slice(uv.bytes())?;
    Ok(secp.sign(&msg, seckey))
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
        let secp = Secp256k1::new();
        verify(&secp, msg, sig, &self.inner)
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
        let secp = Secp256k1::new();
        match sign(&secp, msg, &self.inner) {
            Ok(sig) => Ok(SigValue { inner: sig }),
            Err(err) => Err(err),
        }
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

///基于sepc256k1的签名方法
pub struct KeyPair {
    vpri: PriKey,
    vpub: PubKey,
}

impl Default for KeyPair {
    fn default() -> Self {
        let secp = Secp256k1::new();
        let mut rng = OsRng::new().unwrap();
        let (seckey, pubkey) = secp.generate_keypair(&mut rng);
        KeyPair {
            vpri: PriKey { inner: seckey },
            vpub: PubKey { inner: pubkey },
        }
    }
}

impl KeyPair {
    pub fn get_pubkey(&self) -> &PubKey {
        &self.vpub
    }
    pub fn get_prikey(&self) -> &PriKey {
        &self.vpri
    }
}

#[test]
fn test_signer() {
    let kp = KeyPair::default();
    let kv = kp.get_prikey();
    let pv = kp.get_pubkey();
    let signature = kv.sign("adfs".as_bytes()).unwrap();
    assert!(!pv.verify("adfs1".as_bytes(), &signature).unwrap());
    assert!(pv.verify("adfs".as_bytes(), &signature).unwrap());
}
