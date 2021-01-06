use crate::bytes::{Bytes, WithBytes};
use core::{fmt, str};
use hex::ToHex;
use sha2::{Digest, Sha256};

pub const SIZE: usize = 32;

///double sha256 hasher
#[derive(Debug)]
pub struct U256 {
    inner: [u8; SIZE],
}

impl Default for U256 {
    fn default() -> Self {
        return U256 { inner: [0u8; SIZE] };
    }
}

impl U256 {
    pub fn new(input: &[u8]) -> Self {
        let mut sh = Sha256::new();
        sh.input(input);
        let mut uv = U256::default();
        uv.inner.copy_from_slice(&sh.result());
        let mut sh = Sha256::new();
        sh.input(&uv.inner);
        uv.inner.copy_from_slice(&sh.result());
        return uv;
    }
    pub fn with_bytes(input: [u8; SIZE]) -> Self {
        U256 { inner: input }
    }
    pub fn encode_hex(&self) -> String {
        self.inner.encode_hex()
    }
    pub fn to_bytes(&self) -> &[u8] {
        &self.inner[..]
    }
}

impl WithBytes<U256> for U256 {
    fn with_bytes(bb: &Vec<u8>) -> U256 {
        let mut inner = [0u8; SIZE];
        inner.copy_from_slice(&bb);
        U256 { inner: inner }
    }
}

impl Bytes for U256 {
    fn bytes(&self) -> Vec<u8> {
        self.inner[..].to_vec()
    }
}

//a == b
impl PartialEq for U256 {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

//a == a
impl Eq for U256 {}

impl fmt::LowerHex for U256 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.encode_hex())?;
        Ok(())
    }
}

impl fmt::Display for U256 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::LowerHex::fmt(self, f)
    }
}

#[test]
fn test_sha256() {
    let x = U256::new("21134".as_bytes());
    assert_eq!(
        x.encode_hex(),
        "c116f56090085e70de9ace850de814862f45021e3212cf8848145de4eb2262e1"
    );
    let y = U256::new("12121".as_bytes());
    assert_ne!(x, y);
    let z = U256::new("21134".as_bytes());
    assert_eq!(x, z);
}

#[test]
fn test_wirter_u256() {
    use crate::iobuf::Writer;
    let mut wb = Writer::default();
    let v1 = U256::new("thisi".as_bytes());
    wb.put(&v1);
    let mut rb = wb.reader();
    let v2: U256 = rb.get();
    assert_eq!(v1, v2);
}