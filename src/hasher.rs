use crate::bytes::{Bytes, WithBytes};
use core::{fmt};
use hex::ToHex;
use sha2::{Digest, Sha256};

pub const SIZE: usize = 32;

///double sha256 hasher
#[derive(Debug)]
pub struct Hasher {
    inner: [u8; SIZE],
}

impl Default for Hasher {
    fn default() -> Self {
        return Hasher { inner: [0u8; SIZE] };
    }
}

impl Hasher {
    pub fn new(input: &[u8]) -> Self {
        let mut sh = Sha256::new();
        sh.input(input);
        let mut uv = Hasher::default();
        uv.inner.copy_from_slice(&sh.result());
        let mut sh = Sha256::new();
        sh.input(&uv.inner);
        uv.inner.copy_from_slice(&sh.result());
        return uv;
    }
    pub fn with_bytes(input: [u8; SIZE]) -> Self {
        Hasher { inner: input }
    }
    pub fn encode_hex(&self) -> String {
        self.inner.encode_hex()
    }
    pub fn to_bytes(&self) -> &[u8] {
        &self.inner[..]
    }
}

impl WithBytes<Hasher> for Hasher {
    fn with_bytes(bb: &Vec<u8>) -> Hasher {
        let mut inner = [0u8; SIZE];
        inner.copy_from_slice(&bb);
        Hasher { inner: inner }
    }
}

impl Bytes for Hasher {
    fn bytes(&self) -> Vec<u8> {
        self.inner[..].to_vec()
    }
}

//a == b
impl PartialEq for Hasher {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

//a == a
impl Eq for Hasher {}

impl fmt::LowerHex for Hasher {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.encode_hex())?;
        Ok(())
    }
}

impl fmt::Display for Hasher {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::LowerHex::fmt(self, f)
    }
}

#[test]
fn test_sha256() {
    let x = Hasher::new("21134".as_bytes());
    assert_eq!(
        x.encode_hex(),
        "c116f56090085e70de9ace850de814862f45021e3212cf8848145de4eb2262e1"
    );
    let y = Hasher::new("12121".as_bytes());
    assert_ne!(x, y);
    let z = Hasher::new("21134".as_bytes());
    assert_eq!(x, z);
}

#[test]
fn test_wirter_u256() {
    use crate::iobuf::Writer;
    let mut wb = Writer::default();
    let v1 = Hasher::new("thisi".as_bytes());
    wb.put(&v1);
    let mut rb = wb.reader();
    let v2: Hasher = rb.get();
    assert_eq!(v1, v2);
}