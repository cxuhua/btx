use crate::u160::{SIZE as U160Size, U160};
use crate::u256::{SIZE as U256Size, U256};
use bytes::Buf;
use bytes::BufMut;

pub struct Writer {
    inner: Vec<u8>,
}

pub struct Reader<'a> {
    inner: &'a [u8],
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Reader<'a> {
        Reader { inner: bytes }
    }
    pub fn get_u8(&mut self) -> u8 {
        self.inner.get_u8()
    }
    pub fn get_u16(&mut self) -> u16 {
        self.inner.get_u16_le()
    }
    pub fn get_u32(&mut self) -> u32 {
        self.inner.get_u32_le()
    }
    pub fn get_u64(&mut self) -> u64 {
        self.inner.get_u64_le()
    }
    pub fn get_i8(&mut self) -> i8 {
        self.inner.get_i8()
    }
    pub fn get_i16(&mut self) -> i16 {
        self.inner.get_i16_le()
    }
    pub fn get_i32(&mut self) -> i32 {
        self.inner.get_i32_le()
    }
    pub fn get_i64(&mut self) -> i64 {
        self.inner.get_i64_le()
    }
    pub fn bytes(&self) -> &[u8] {
        self.inner
    }
    pub fn get_u256(&mut self) -> U256 {
        let mut dst = [0u8; U256Size];
        self.inner.copy_to_slice(&mut dst);
        U256::with_bytes(dst)
    }
    pub fn get_u160(&mut self) -> U160 {
        let mut dst = [0u8; U160Size];
        self.inner.copy_to_slice(&mut dst);
        U160::with_bytes(dst)
    }
}

impl Default for Writer {
    fn default() -> Self {
        return Writer {
            inner: Vec::with_capacity(32),
        };
    }
}

impl Writer {
    pub fn new(cap: usize) -> Self {
        Writer {
            inner: Vec::with_capacity(cap),
        }
    }
    pub fn reader(&self) -> Reader {
        Reader {
            inner: &self.inner[..],
        }
    }
    pub fn put_u160(&mut self, v: &U160) {
        self.inner.put(v.bytes());
    }
    pub fn put_u256(&mut self, v: &U256) {
        self.inner.put(v.bytes());
    }
    pub fn put_writer(&mut self, w: &Writer) {
        self.inner.put(w.reader().bytes());
    }
    pub fn put_u8(&mut self, v: u8) {
        self.inner.put_u8(v);
    }
    pub fn put_u16(&mut self, v: u16) {
        self.inner.put_u16_le(v);
    }
    pub fn put_u32(&mut self, v: u32) {
        self.inner.put_u32_le(v);
    }
    pub fn put_u64(&mut self, v: u64) {
        self.inner.put_u64_le(v);
    }
    pub fn put_i8(&mut self, v: i8) {
        self.inner.put_i8(v);
    }
    pub fn put_i16(&mut self, v: i16) {
        self.inner.put_i16_le(v);
    }
    pub fn put_i32(&mut self, v: i32) {
        self.inner.put_i32_le(v);
    }
    pub fn put_i64(&mut self, v: i64) {
        self.inner.put_i64_le(v);
    }
}


#[test]
fn test_wirter_u256() {
    let mut wb = Writer::default();
    let v1 = U256::new("thisi".as_bytes());
    wb.put_u256(&v1);
    let mut rb = wb.reader();
    let v2 = rb.get_u256();
    assert_eq!(v1, v2);
}

#[test]
fn test_wirter() {
    let mut wb = Writer::default();
    let v1 = 0x11223344;
    let v2 = 0x44332211;
    //write
    wb.put_u32(v1);
    wb.put_u32(v2);
    //read
    let mut rb = wb.reader();
    assert_eq!(rb.bytes(), [68, 51, 34, 17, 17, 34, 51, 68]);
    let c1 = rb.get_u32();
    let c2 = rb.get_u32();
    assert_eq!(v1, c1);
    assert_eq!(v2, c2);
}