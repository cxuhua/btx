use crate::bytes::{Bytes, WithBytes};
use core::{fmt, str};
use hex::ToHex;
use ripemd160::{Digest, Ripemd160};

pub const SIZE: usize = 20;

#[derive(Debug)]
pub struct U160 {
    inner: [u8; SIZE],
}

impl Default for U160 {
    fn default() -> Self {
        return U160 { inner: [0u8; SIZE] };
    }
}

impl U160 {
    pub fn new(input: &[u8]) -> Self {
        let mut sh = Ripemd160::default();
        sh.update(input);
        let mut uv = U160::default();
        uv.inner.copy_from_slice(&sh.finalize());
        return uv;
    }
    pub fn encode_hex(&self) -> String {
        self.inner.encode_hex()
    }
    pub fn with_bytes(input: [u8; SIZE]) -> Self {
        U160 { inner: input }
    }
    pub fn to_bytes(&self) -> &[u8] {
        &self.inner[..]
    }
}

impl WithBytes<U160> for U160 {
    fn with_bytes(bb: &Vec<u8>) -> U160 {
        let mut inner = [0u8; SIZE];
        inner.copy_from_slice(&bb);
        U160 { inner: inner }
    }
}

impl Bytes for U160 {
    fn bytes(&self) -> Vec<u8> {
        self.inner[..].to_vec()
    }
}

//a == b
impl PartialEq for U160 {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

//a == a
impl Eq for U160 {}

impl fmt::LowerHex for U160 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.encode_hex())?;
        Ok(())
    }
}

impl fmt::Display for U160 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::LowerHex::fmt(self, f)
    }
}

#[test]
fn test_ripemd160() {
    let x = U160::new("21134".as_bytes());
    assert_eq!(x.encode_hex(), "0bf6b68f1ca777d5312b795d104d0a72ba48f071");
    let y = U160::new("12121".as_bytes());
    assert_ne!(x, y);
    let z = U160::new("21134".as_bytes());
    assert_eq!(x, z);
}

#[test]
fn test_wirter_u160() {
    use crate::iobuf::Writer;
    let mut wb = Writer::default();
    let v1 = U160::new("thasdsdisi".as_bytes());
    wb.put(&v1);
    let mut rb = wb.reader();
    let v2: U160 = rb.get();
    assert_eq!(v1, v2);
}