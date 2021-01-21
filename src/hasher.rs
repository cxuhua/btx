use crate::bytes::{Bytes, WithBytes};
use crate::errors;
use core::fmt;
use hex::{FromHex, ToHex};
use sha2::{Digest, Sha256};
pub const SIZE: usize = 32;

use num_bigint::BigUint;
use num_traits::FromPrimitive;
use std::cmp::Ordering;
use std::ops::{Div, Mul};

#[test]
fn test_compact() {
    let h1 = Hasher::from_compact(0x170da8a1).unwrap();
    assert_eq!(h1.compact(), 0x170da8a1);

    let h = Hasher::from_compact(0x1d00ffff).unwrap();
    assert_eq!(h.compact(), 0x1d00ffff);

    let h = Hasher::from_compact(0x1b04864c).unwrap();
    assert_eq!(h.compact(), 0x1b04864c);

    let h = Hasher::from_compact(0x1a05db8b).unwrap();
    assert_eq!(h.compact(), 0x1a05db8b);

    let h = Hasher::from_compact(0x18009645).unwrap();
    assert_eq!(h.compact(), 0x18009645);
}

#[test]
fn test_pow() {
    let limit =
        Hasher::from_str("00000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffff")
            .unwrap();
    assert_eq!(limit.compact(), 0x1d00ffff);

    let hash = Hasher::from_str("0000000000000000000e20e727e0f9e4d88c44d68e572fbc9a2bd8c61e50010b")
        .unwrap();
    assert!(hash.check_pow(&limit, 0x1715b23e));

    let hash = Hasher::from_str("000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f")
        .unwrap();
    assert!(hash.check_pow(&limit, 0x1d00ffff));
}

#[test]
fn test_compute_bits() {
    let limit =
        Hasher::from_str("00000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffff")
            .unwrap();
    let bits = limit.compute_bits(1209600, 1349255821, 1348121651, 0x1a05db8b);
    assert_eq!(bits, 0x1a057e08);
}

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
    ///计算下个工作难度
    /// self: 最小工作难度
    /// stime : 时间间隔 默认:14 * 24 * 60 * 60 = 1209600 每14天2016个的速度
    /// ltime : 最后一个区块的时间
    /// ntime : (最后一个区块的高度 - 2016 + 1)区块的时间
    /// lpow : 最后一个区块的工作难度
    pub fn compute_bits(&self, stime: u32, ltime: u32, ntime: u32, lpow: u32) -> u32 {
        debug_assert!(stime > 0);
        debug_assert!(ltime > ntime);
        let mut sub = ltime - ntime;
        let sv = stime / 4;
        if sub < sv {
            sub = sv;
        }
        let sv = stime * 4;
        if sub > sv {
            sub = sv;
        }
        if let Ok(pow) = Hasher::from_compact(lpow) {
            let pow = pow * sub;
            let pow = pow / stime;
            if &pow > self {
                return self.compact();
            }
            return pow.compact();
        }
        return self.compact();
    }
    ///工作量证明检测
    pub fn check_pow(&self, limit: &Hasher, bits: u32) -> bool {
        if let Ok(v) = Hasher::from_compact(bits) {
            //如果比最小难度大
            if &v > limit {
                return false;
            }
            return self <= &v;
        }
        false
    }
    fn low64(v: &Vec<u32>) -> u64 {
        if v.len() == 0 {
            return 0;
        }
        if v.len() == 1 {
            return v[0] as u64;
        }
        return (v[0] as u64) | (v[1] as u64) << 32;
    }
    pub fn compact(&self) -> u32 {
        let b = self.to_biguint();
        let mut s = (b.bits() + 7) / 8;
        let mut cv: u64 = 0;
        if s <= 3 {
            let low64 = Self::low64(&b.to_u32_digits());
            cv = low64 << (8 * (3 - s));
        } else {
            let b = b >> 8 * (s - 3);
            cv = Self::low64(&b.to_u32_digits())
        }
        if cv & 0x00800000 != 0 {
            cv >>= 8;
            s += 1;
        }
        cv |= s << 24;
        return cv as u32;
    }
    pub fn from_compact(v: u32) -> Result<Hasher, errors::Error> {
        let s = v >> 24;
        let mut w = v & 0x007fffff;
        if s <= 3 {
            w >>= 8 * (3 - s);
            if let Some(v) = BigUint::from_u32(w) {
                return Ok(Hasher::from_biguint(&v));
            }
        } else {
            if let Some(v) = BigUint::from_u32(w) {
                let v = v << 8 * (s - 3);
                return Ok(Hasher::from_biguint(&v));
            }
        }
        Err(errors::Error::InvalidParam)
    }
    pub fn from_str(v: &str) -> Result<Self, hex::FromHexError> {
        let b: Vec<u8> = Vec::from_hex(v.as_bytes())?;
        let mut inner = [0u8; SIZE];
        let idx = if b.len() > SIZE { 0 } else { SIZE - b.len() };
        inner[idx..].copy_from_slice(&b);
        inner.reverse();
        Ok(Hasher { inner: inner })
    }
    fn to_biguint(&self) -> BigUint {
        BigUint::from_bytes_le(&self.inner[..])
    }
    fn from_biguint(v: &BigUint) -> Self {
        let bb = v.to_bytes_be();
        let mut inner = [0u8; SIZE];
        let idx = if bb.len() > SIZE { 0 } else { SIZE - bb.len() };
        inner[idx..].clone_from_slice(&bb);
        inner.reverse();
        Hasher { inner: inner }
    }
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
        let mut mi = [0u8; SIZE];
        mi.copy_from_slice(&self.inner);
        mi.reverse();
        mi.encode_hex()
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

impl PartialOrd for Hasher {
    #[inline]
    fn partial_cmp(&self, other: &Hasher) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Hasher {
    #[inline]
    fn cmp(&self, other: &Hasher) -> Ordering {
        let l = self.to_biguint();
        let r = other.to_biguint();
        return l.cmp(&r);
    }
}

impl Div<u32> for Hasher {
    type Output = Hasher;

    #[inline]
    fn div(self, other: u32) -> Hasher {
        let b = self.to_biguint() / other;
        Hasher::from_biguint(&b)
    }
}

impl Mul<u32> for Hasher {
    type Output = Hasher;

    #[inline]
    fn mul(self, other: u32) -> Hasher {
        let b = self.to_biguint() * other;
        Hasher::from_biguint(&b)
    }
}

#[test]
fn test_sha256() {
    let x = Hasher::new("21134".as_bytes());
    assert_eq!(
        x.encode_hex(),
        "e16222ebe45d144888cf12321e02452f8614e80d85ce9ade705e089060f516c1"
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
