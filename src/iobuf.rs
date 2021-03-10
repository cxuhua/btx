use crate::bytes::{FromBytes, IntoBytes};
use crate::errors;
use bytes::Buf;
use bytes::BufMut;

/// 二机制生成
pub trait Serializer: Sized {
    /// 编码数据到writer
    fn encode(&self, w: &mut Writer);
    /// 从reader读取数据
    fn decode(r: &mut Reader) -> Result<Self, errors::Error>
    where
        Self: Default;
}

#[derive(Debug)]
pub struct Writer {
    inner: Vec<u8>,
}

impl Clone for Writer {
    fn clone(&self) -> Self {
        Writer {
            inner: self.inner.clone(),
        }
    }
}

#[derive(Debug)]
pub struct Reader<'a> {
    inner: &'a [u8],
}

impl<'a> Reader<'a> {
    /// 检测剩余字节必须>=l并返回剩余字节
    fn check(&self, l: usize) -> Result<usize, errors::Error> {
        let rl = self.remaining();
        if rl < l {
            Err(errors::Error::IoBufReadErr)
        } else {
            Ok(rl)
        }
    }
    pub fn usize(&mut self) -> Result<usize, errors::Error> {
        self.check(1)?;
        let b = self.u8()?;
        if b & 0x80 == 0 {
            return Ok(b as usize);
        }
        let b1 = b & 0x7F;
        self.check(1)?;
        let b2 = self.u8()?;
        let size = ((b1 as usize) << 8) | (b2 as usize);
        return Ok(size);
    }
    // 剩余字节数
    pub fn remaining(&self) -> usize {
        self.inner.remaining()
    }
    pub fn new(bytes: &'a [u8]) -> Reader<'a> {
        Reader { inner: bytes }
    }
    pub fn u8(&mut self) -> Result<u8, errors::Error> {
        self.check(1)?;
        Ok(self.inner.get_u8())
    }
    /// 推进读取位置cnt长度
    pub fn advance(&mut self, cnt: usize) -> Result<(), errors::Error> {
        self.check(cnt)?;
        self.inner.advance(cnt);
        Ok(())
    }
    //动态获取根据size确定字节长度
    pub fn length(&mut self, size: usize) -> Result<usize, errors::Error> {
        self.check(size)?;
        match size {
            1 => Ok(self.u8()? as usize),
            2 => Ok(self.u16()? as usize),
            3 => Ok(self.u32()? as usize),
            _ => Err(errors::Error::IoBufReadErr),
        }
    }
    pub fn u16(&mut self) -> Result<u16, errors::Error> {
        self.check(2)?;
        Ok(self.inner.get_u16_le())
    }
    pub fn u32(&mut self) -> Result<u32, errors::Error> {
        self.check(4)?;
        Ok(self.inner.get_u32_le())
    }
    pub fn u64(&mut self) -> Result<u64, errors::Error> {
        self.check(8)?;
        Ok(self.inner.get_u64_le())
    }
    pub fn i8(&mut self) -> Result<i8, errors::Error> {
        self.check(1)?;
        Ok(self.inner.get_i8())
    }
    pub fn i16(&mut self) -> Result<i16, errors::Error> {
        self.check(2)?;
        Ok(self.inner.get_i16_le())
    }
    pub fn i32(&mut self) -> Result<i32, errors::Error> {
        self.check(4)?;
        Ok(self.inner.get_i32_le())
    }
    pub fn i64(&mut self) -> Result<i64, errors::Error> {
        self.check(8)?;
        Ok(self.inner.get_i64_le())
    }
    // 获取所有数据
    pub fn bytes(&self) -> &[u8] {
        self.inner
    }
    // 获取指定长度的数据
    pub fn get_bytes(&mut self, size: usize) -> Result<Vec<u8>, errors::Error> {
        self.check(size)?;
        let mut vp: Vec<u8> = Vec::with_capacity(size);
        unsafe {
            vp.set_len(size);
        }
        self.inner.copy_to_slice(vp.as_mut());
        Ok(vp)
    }
    /// use put放入的使用get取出
    /// 先取出数据长度,在获取数据内容
    pub fn get<T>(&mut self) -> Result<T, errors::Error>
    where
        T: FromBytes,
    {
        let size = self.usize()?;
        let mut vp: Vec<u8> = Vec::with_capacity(size);
        unsafe {
            vp.set_len(size);
        }
        self.check(size)?;
        self.inner.copy_to_slice(vp.as_mut());
        T::from_bytes(&vp)
    }
    /// 从Reader中读取T类型数据,使用默认的方法创建对象返回
    pub fn decode<T>(&mut self) -> Result<T, errors::Error>
    where
        T: Serializer + Default,
    {
        T::decode(self)
    }
}

//a == b
impl PartialEq for Writer {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

//a == a
impl Eq for Writer {}

impl Default for Writer {
    fn default() -> Self {
        Writer::new(32)
    }
}

impl IntoBytes for Writer {
    fn into_bytes(&self) -> Vec<u8> {
        self.inner.to_vec()
    }
}

impl FromBytes for Writer {
    fn from_bytes(bb: &Vec<u8>) -> Result<Self, errors::Error> {
        Ok(Writer { inner: bb.clone() })
    }
}

impl Writer {
    // 把类型T编码写入writer
    pub fn encode<T>(&mut self, v: &T)
    where
        T: Serializer,
    {
        v.encode(self);
    }
    // 动态整数 支持0..=0x7FFF
    pub fn usize(&mut self, size: usize) {
        if size <= 0x7F {
            self.u8(size as u8);
        } else if size <= 0x7FFF {
            let low = (size & 0xFF) as u8;
            let high = (size >> 8) & 0xFF;
            let high = high | 0x80;
            self.u8(high as u8);
            self.u8(low as u8);
        } else {
            assert!(size <= 0x7FFF);
        }
    }
    /// 当内容长度不固定时使用此方法放置内容长度
    pub fn put<T>(&mut self, v: &T)
    where
        T: IntoBytes,
    {
        let bb = v.into_bytes();
        self.usize(bb.len());
        self.inner.put(&bb[..])
    }
    pub fn new(cap: usize) -> Self {
        Writer {
            inner: Vec::with_capacity(cap),
        }
    }
    pub fn len(&self) -> usize {
        self.inner.len()
    }
    pub fn bytes(&self) -> &[u8] {
        &self.inner[..]
    }
    pub fn reader(&self) -> Reader {
        Reader {
            inner: &self.inner[..],
        }
    }
    pub fn put_writer(&mut self, b: &Writer) {
        self.put_bytes(b.bytes());
    }
    pub fn put_bytes(&mut self, b: &[u8]) {
        self.inner.put(b);
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
fn test_usize() {
    let mut wb = Writer::default();
    wb.usize(0xFF1);
    wb.usize(0x60);
    wb.usize(0x0);
    wb.usize(0x7FFF);
    let mut rb = wb.reader();
    assert_eq!(rb.remaining(), 6);
    assert_eq!(rb.usize().unwrap(), 0xFF1);
    assert_eq!(rb.usize().unwrap(), 0x60);
    assert_eq!(rb.usize().unwrap(), 0x0);
    assert_eq!(rb.usize().unwrap(), 0x7FFF);
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
    let c1 = rb.u32().unwrap();
    let c2 = rb.u32().unwrap();
    assert_eq!(v1, c1);
    assert_eq!(v2, c2);
}
