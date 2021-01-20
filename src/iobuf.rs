use crate::bytes::{Bytes, WithBytes};
use crate::errors;
use bytes::Buf;
use bytes::BufMut;

pub struct Writer {
    inner: Vec<u8>,
}

pub struct Reader<'a> {
    inner: &'a [u8],
}

impl<'a> Reader<'a> {
    ///检测剩余字节必须>=l并返回剩余字节
    pub fn check(&self, l: usize) -> Result<usize, errors::Error> {
        let rl = self.remaining();
        if rl < l {
            Err(errors::Error::ScriptFmtErr)
        } else {
            Ok(rl)
        }
    }
    //剩余字节数
    pub fn remaining(&self) -> usize {
        self.inner.remaining()
    }
    pub fn new(bytes: &'a [u8]) -> Reader<'a> {
        Reader { inner: bytes }
    }
    pub fn u8(&mut self) -> u8 {
        self.inner.get_u8()
    }
    pub fn u16(&mut self) -> u16 {
        self.inner.get_u16_le()
    }
    pub fn u32(&mut self) -> u32 {
        self.inner.get_u32_le()
    }
    pub fn u64(&mut self) -> u64 {
        self.inner.get_u64_le()
    }
    pub fn i8(&mut self) -> i8 {
        self.inner.get_i8()
    }
    pub fn i16(&mut self) -> i16 {
        self.inner.get_i16_le()
    }
    pub fn i32(&mut self) -> i32 {
        self.inner.get_i32_le()
    }
    pub fn i64(&mut self) -> i64 {
        self.inner.get_i64_le()
    }
    pub fn bytes(&self) -> &[u8] {
        self.inner
    }
    pub fn get_bytes(&mut self, size: usize) -> Vec<u8> {
        let mut vp: Vec<u8> = Vec::with_capacity(size);
        unsafe {
            vp.set_len(size);
        }
        self.inner.copy_to_slice(vp.as_mut());
        vp
    }
    //长度限制到最大255
    pub fn get<T>(&mut self) -> T
    where
        T: WithBytes<T>,
    {
        let size = self.u8() as usize;
        let mut vp: Vec<u8> = Vec::with_capacity(size);
        unsafe {
            vp.set_len(size);
        }
        self.inner.copy_to_slice(vp.as_mut());
        T::with_bytes(&vp)
    }
}

impl Default for Writer {
    fn default() -> Self {
        Writer::new(32)
    }
}

impl Writer {
    pub fn put<T>(&mut self, v: &T)
    where
        T: Bytes,
    {
        let bb = v.bytes();
        assert!(bb.len() <= 0xFF);
        self.u8(bb.len() as u8);
        self.inner.put(&bb[..])
    }
    pub fn new(cap: usize) -> Self {
        Writer {
            inner: Vec::with_capacity(cap),
        }
    }
    pub fn bytes(&self) -> &[u8] {
        &self.inner[..]
    }
    pub fn reader(&self) -> Reader {
        Reader {
            inner: &self.inner[..],
        }
    }
    pub fn u8(&mut self, v: u8) {
        self.inner.put_u8(v);
    }
    pub fn u16(&mut self, v: u16) {
        self.inner.put_u16_le(v);
    }
    pub fn u32(&mut self, v: u32) {
        self.inner.put_u32_le(v);
    }
    pub fn u64(&mut self, v: u64) {
        self.inner.put_u64_le(v);
    }
    pub fn i8(&mut self, v: i8) {
        self.inner.put_i8(v);
    }
    pub fn i16(&mut self, v: i16) {
        self.inner.put_i16_le(v);
    }
    pub fn i32(&mut self, v: i32) {
        self.inner.put_i32_le(v);
    }
    pub fn i64(&mut self, v: i64) {
        self.inner.put_i64_le(v);
    }
}

#[test]
fn test_wirter() {
    let mut wb = Writer::default();
    let v1 = 0x11223344;
    let v2 = 0x44332211;
    //write
    wb.u32(v1);
    wb.u32(v2);
    //read
    let mut rb = wb.reader();
    assert_eq!(rb.bytes(), [68, 51, 34, 17, 17, 34, 51, 68]);
    let c1 = rb.u32();
    let c2 = rb.u32();
    assert_eq!(v1, c1);
    assert_eq!(v2, c2);
}
