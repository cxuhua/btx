use crate::consts;
use crate::errors;
use crate::iobuf;
use bytes::BufMut;
use std::convert::{From, Into, TryFrom, TryInto};

/// 放入0 - 16
pub const OP_00: u8 = 0x00;
pub const OP_01: u8 = 0x01;
pub const OP_02: u8 = 0x02;
pub const OP_03: u8 = 0x03;
pub const OP_04: u8 = 0x04;
pub const OP_05: u8 = 0x05;
pub const OP_06: u8 = 0x06;
pub const OP_07: u8 = 0x07;
pub const OP_08: u8 = 0x08;
pub const OP_09: u8 = 0x09;
pub const OP_10: u8 = 0x0a;
pub const OP_11: u8 = 0x0b;
pub const OP_12: u8 = 0x0c;
pub const OP_13: u8 = 0x0d;
pub const OP_14: u8 = 0x0e;
pub const OP_15: u8 = 0x0f;
pub const OP_16: u8 = 0x10;
/// 放入false true
pub const OP_FALSE: u8 = 0x90;
pub const OP_TRUE: u8 = 0x91;
/// 放入8位的数字 i8
pub const OP_NUMBER_1: u8 = 0xA0;
/// 放入16位的数字 i16
pub const OP_NUMBER_2: u8 = 0xA1;
/// 放入32位的数字 i32
pub const OP_NUMBER_4: u8 = 0xA2;
/// 放入64位的数字 i64
pub const OP_NUMBER_8: u8 = 0xA3;

/// 推送数据到堆栈,数据长度为1个字节
pub const OP_DATA_1: u8 = 0xB0;
/// 推送数据到堆栈,数据长度为2个字节
pub const OP_DATA_2: u8 = 0xB1;
/// 推送数据到堆栈,数据长度为4个字节
pub const OP_DATA_4: u8 = 0xB2;

///验证栈顶是否为 OP_TRUE 不是就立即结束返回并丢弃栈顶的参数
pub const OP_VERIFY: u8 = 0xC0;
///检测数据是否相等 类型和数据都必须相等 结果放入栈顶 top(-1) == top(-2) -> push bool,丢下使用参数
pub const OP_EQUAL: u8 = 0xC1;
///取反true <-> false
pub const OP_NOT: u8 = 0xC2;
///检测签名是否正确 栈顶放入签名数据,验证成功将一个bool放入栈顶
pub const OP_CHECKSIG: u8 = 0xC3;

//脚本执行器
#[derive(Debug)]
struct Exector {
    eles: Vec<Ele>,
}

impl Exector {
    //
    fn len(&self) -> usize {
        self.eles.len()
    }
    ///获取栈顶元素
    /// s3  -1 +3
    /// s2  -2 +2
    /// s1  -3 +1
    fn top(&self, n: isize) -> &Ele {
        let l = self.len() as isize;
        if n < 0 {
            &self.eles[(l + n) as usize]
        } else {
            &self.eles[(n - 1) as usize]
        }
    }
    fn pop(&mut self, n: usize) -> Result<(), errors::Error> {
        if self.eles.len() < n {
            return Err(errors::Error::StackLenErr);
        }
        self.eles.truncate(self.eles.len() - n);
        Ok(())
    }
    fn new() -> Exector {
        Exector { eles: vec![] }
    }
    //检测栈元素数量
    fn check(&self, l: usize) -> Result<usize, errors::Error> {
        let rl = self.len();
        if rl < l {
            Err(errors::Error::StackLenErr)
        } else {
            Ok(rl)
        }
    }
    ///执行脚本
    fn exec(&mut self, script: &Script) -> Result<usize, errors::Error> {
        let mut reader = iobuf::Reader::new(script.to_bytes());
        if reader.remaining() == 0 {
            return Err(errors::Error::ScriptEmptyErr);
        }
        let mut step = 0;
        loop {
            step += 1;
            let op = reader.u8();
            match op {
                OP_TRUE | OP_FALSE => {
                    self.eles.push(Ele::from(op == OP_TRUE));
                }
                OP_00..=OP_16 => {
                    self.eles.push(Ele::from(op as i64));
                }
                OP_NUMBER_1..=OP_NUMBER_8 => match op {
                    OP_NUMBER_1 => {
                        reader.check(1)?;
                        let v = reader.i8() as i64;
                        self.eles.push(Ele::from(v));
                    }
                    OP_NUMBER_2 => {
                        reader.check(2)?;
                        let v = reader.i16() as i64;
                        self.eles.push(Ele::from(v));
                    }
                    OP_NUMBER_4 => {
                        reader.check(4)?;
                        let v = reader.i32() as i64;
                        self.eles.push(Ele::from(v));
                    }
                    OP_NUMBER_8 => {
                        reader.check(8)?;
                        let v = reader.i64() as i64;
                        self.eles.push(Ele::from(v));
                    }
                    _ => return Err(errors::Error::ScriptFmtErr),
                },
                OP_DATA_1..=OP_DATA_4 => match op {
                    OP_DATA_1 => {
                        reader.check(1)?;
                        let l = reader.u8() as usize;
                        reader.check(l)?;
                        let d = reader.get_bytes(l);
                        self.eles.push(Ele::from(d));
                    }
                    OP_DATA_2 => {
                        reader.check(2)?;
                        let l = reader.u16() as usize;
                        reader.check(l)?;
                        let d = reader.get_bytes(l);
                        self.eles.push(Ele::from(d));
                    }
                    OP_DATA_4 => {
                        reader.check(4)?;
                        let l = reader.u32() as usize;
                        reader.check(l)?;
                        let d = reader.get_bytes(l);
                        self.eles.push(Ele::from(d));
                    }
                    _ => return Err(errors::Error::ScriptFmtErr),
                },
                OP_VERIFY => {
                    //验证顶部元素是否为true,不是返回错误,负责删除栈顶继续
                    self.check(1)?;
                    let val: bool = self.top(-1).try_into()?;
                    self.pop(1)?;
                    if !val {
                        return Err(errors::Error::ScriptVerifyErr);
                    }
                }
                OP_EQUAL => {
                    //比较栈顶的两个元素是否相等
                    self.check(2)?;
                    let l = self.top(-1);
                    let r = self.top(-2);
                    let b = l == r;
                    self.pop(2)?;
                    self.eles.push(Ele::from(b));
                }
                OP_NOT => {
                    //对栈顶的bool值取反
                    self.check(1)?;
                    let val: bool = self.top(-1).try_into()?;
                    self.pop(1)?;
                    self.eles.push(Ele::from(!val));
                }
                OP_CHECKSIG => {
                    //检测签名,放置结果到栈顶并销毁参数数据
                }
                _ => {
                    return Err(errors::Error::ScriptFmtErr);
                }
            }
            if step > consts::MAX_SCRIPT_OPS {
                return Err(errors::Error::ScriptFmtErr);
            }
            if reader.remaining() == 0 {
                break;
            }
        }
        Ok(step)
    }
}

#[test]
fn test_op_not() {
    let mut script = Script::new(32);
    script.bool(false);
    script.op(OP_NOT);
    let mut exector = Exector::new();
    exector.exec(&script).unwrap();
    assert_eq!(exector.len(), 1);
    let b: bool = exector.top(-1).try_into().unwrap();
    assert_eq!(b, true);

    let mut script = Script::new(32);
    script.bool(true);
    script.op(OP_NOT);
    let mut exector = Exector::new();
    exector.exec(&script).unwrap();
    assert_eq!(exector.len(), 1);
    let b: bool = exector.top(-1).try_into().unwrap();
    assert_eq!(b, false);
}

#[test]
fn test_op_equal() {
    let mut script = Script::new(32);
    script.i8(1);
    script.i16(1);
    script.op(OP_EQUAL);
    let mut exector = Exector::new();
    exector.exec(&script).unwrap();
    assert_eq!(exector.len(), 1);
    let b: bool = exector.top(-1).try_into().unwrap();
    assert_eq!(b, true);

    let mut script = Script::new(32);
    script.op(OP_01);
    script.i16(1);
    script.op(OP_EQUAL);
    let mut exector = Exector::new();
    exector.exec(&script).unwrap();
    assert_eq!(exector.len(), 1);
    let b: bool = exector.top(-1).try_into().unwrap();
    assert_eq!(b, true);

    let mut script = Script::new(32);
    script.string(&String::from("111"));
    script.string(&String::from("222"));
    script.op(OP_EQUAL);
    let mut exector = Exector::new();
    exector.exec(&script).unwrap();
    assert_eq!(exector.len(), 1);
    let b: bool = exector.top(-1).try_into().unwrap();
    assert_eq!(b, false);

    let mut script = Script::new(32);
    script.string(&String::from("111"));
    script.string(&String::from("222"));
    script.op(OP_EQUAL);
    script.bool(false);
    script.op(OP_EQUAL);
    let mut exector = Exector::new();
    exector.exec(&script).unwrap();
    assert_eq!(exector.len(), 1);
    let b: bool = exector.top(-1).try_into().unwrap();
    assert_eq!(b, true);
}

#[test]
fn test_op_verify() {
    let mut script = Script::new(32);
    script.bool(true);
    script.op(OP_VERIFY);
    let mut exector = Exector::new();
    exector.exec(&script).unwrap();
    assert_eq!(exector.len(), 0);
}

#[test]
fn test_push_data() {
    let mut script = Script::new(32);
    script.data(&[1, 2, 3]);
    script.data(&[10, 20, 30]);
    let mut exector = Exector::new();
    exector.exec(&script).unwrap();
    assert_eq!(exector.len(), 2);
    let d1: &[u8] = exector.top(1).try_into().unwrap();
    assert_eq!(d1, [1, 2, 3]);
    let d2: &[u8] = exector.top(2).try_into().unwrap();
    assert_eq!(d2, [10, 20, 30]);
}

#[test]
fn test_push_number() {
    let mut script = Script::new(32);
    script.i8(8);
    script.i16(16);
    script.i32(32);
    script.i64(64);
    let mut exector = Exector::new();
    exector.exec(&script).unwrap();
    assert_eq!(exector.len(), 4);
    let d1: i64 = exector.top(1).try_into().unwrap();
    let d2: i64 = exector.top(2).try_into().unwrap();
    let d3: i64 = exector.top(3).try_into().unwrap();
    let d4: i64 = exector.top(4).try_into().unwrap();
    assert_eq!(d1, 8);
    assert_eq!(d2, 16);
    assert_eq!(d3, 32);
    assert_eq!(d4, 64);
}

#[test]
fn test_push_number_0_16() {
    let mut script = Script::new(32);
    for v in 0..=16 {
        script.op(v as u8);
    }
    let mut exector = Exector::new();
    exector.exec(&script).unwrap();
    assert_eq!(exector.len(), 17);
    for v in 1..=17 {
        let d1: i64 = exector.top(v).try_into().unwrap();
        assert_eq!(d1, (v - 1) as i64);
    }
}

#[test]
fn test_push_bool() {
    let mut script = Script::new(32);
    script.bool(true);
    script.bool(false);
    let mut exector = Exector::new();
    exector.exec(&script).unwrap();
    assert_eq!(exector.len(), 2);
    let d1: bool = exector.top(-1).try_into().unwrap();
    assert_eq!(d1, false);
    let d1: bool = exector.top(-2).try_into().unwrap();
    assert_eq!(d1, true);
    let d1: bool = exector.top(1).try_into().unwrap();
    assert_eq!(d1, true);
    let d1: bool = exector.top(2).try_into().unwrap();
    assert_eq!(d1, false);
}

///脚本生成
pub struct Script {
    inner: Vec<u8>,
}

impl Default for Script {
    fn default() -> Self {
        Script::new(256)
    }
}

impl Script {
    ///获取脚本内容
    pub fn to_bytes(&self) -> &[u8] {
        return &self.inner[..];
    }
    pub fn new(cap: usize) -> Self {
        Script {
            inner: Vec::with_capacity(cap),
        }
    }
    ///链接另外一个脚本
    pub fn concat(&mut self, script: &Script) -> &mut Self {
        self.inner.put(script.to_bytes());
        return self;
    }
    ///push bool
    pub fn bool(&mut self, v: bool) -> &mut Self {
        let vv: u8 = if v { OP_TRUE } else { OP_FALSE };
        self.op(vv);
        return self;
    }
    ///push op
    pub fn op(&mut self, op: u8) -> &mut Self {
        self.inner.put_u8(op);
        return self;
    }
    ///push string
    pub fn string(&mut self, s: &String) -> &mut Self {
        self.data(s.as_bytes());
        return self;
    }
    ///push binary
    pub fn data(&mut self, v: &[u8]) -> &mut Self {
        let l = v.len();
        if l <= 0xFF {
            self.op(OP_DATA_1);
            self.inner.put_u8(l as u8);
        } else if l <= 0xFFFF {
            self.op(OP_DATA_2);
            self.inner.put_u16(l as u16);
        } else if l <= 0xFFFFFFFF {
            self.op(OP_DATA_4);
            self.inner.put_u32(l as u32);
        }
        self.inner.put(v);
        return self;
    }
    ///push number
    pub fn i8(&mut self, v: i8) -> &mut Self {
        self.op(OP_NUMBER_1);
        self.inner.put_i8(v);
        return self;
    }
    ///push number
    pub fn i16(&mut self, v: i16) -> &mut Self {
        self.op(OP_NUMBER_2);
        self.inner.put_i16_le(v);
        return self;
    }
    ///push number
    pub fn i32(&mut self, v: i32) -> &mut Self {
        self.op(OP_NUMBER_4);
        self.inner.put_i32_le(v);
        return self;
    }
    ///push number
    pub fn i64(&mut self, v: i64) -> &mut Self {
        self.op(OP_NUMBER_8);
        self.inner.put_i64_le(v);
        return self;
    }
}
///栈元素
#[derive(Debug)]
enum Ele {
    Bool(bool),
    Number(i64),
    Data(Vec<u8>),
}

impl TryFrom<&Ele> for bool {
    type Error = errors::Error;
    fn try_from(value: &Ele) -> Result<Self, Self::Error> {
        if let Ele::Bool(pv) = value {
            return Ok(*pv);
        }
        return Err(errors::Error::StackEleTypeErr);
    }
}

impl TryFrom<&Ele> for i64 {
    type Error = errors::Error;
    fn try_from(value: &Ele) -> Result<Self, Self::Error> {
        if let Ele::Number(pv) = value {
            return Ok(*pv);
        }
        return Err(errors::Error::StackEleTypeErr);
    }
}

impl<'a> TryFrom<&'a Ele> for &'a [u8] {
    type Error = errors::Error;
    fn try_from(value: &'a Ele) -> Result<Self, Self::Error> {
        if let Ele::Data(pv) = value {
            return Ok(pv);
        }
        return Err(errors::Error::StackEleTypeErr);
    }
}

impl From<bool> for Ele {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}

impl From<i64> for Ele {
    fn from(v: i64) -> Self {
        Self::Number(v)
    }
}

impl From<Vec<u8>> for Ele {
    fn from(v: Vec<u8>) -> Self {
        Self::Data(v)
    }
}

impl From<&Vec<u8>> for Ele {
    fn from(v: &Vec<u8>) -> Self {
        Self::Data(v.clone())
    }
}

impl PartialEq for Ele {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Bool(l), Self::Bool(r)) => l == r,
            (Self::Number(l), Self::Number(r)) => l == r,
            (Self::Data(l), Self::Data(r)) => l == r,
            _ => false,
        }
    }
}

#[test]
fn test_ele_equ() {
    let bl = Ele::from(true);
    let br = Ele::from(true);
    let bv = Ele::from(false);
    assert_eq!(bl, bl);
    assert_eq!(bl, br);
    assert_ne!(bv, bl);
    assert_ne!(bv, br);

    let il = Ele::from(1);
    let ir = Ele::from(1);
    let iv = Ele::from(123);
    assert_eq!(il, il);
    assert_eq!(il, ir);
    assert_ne!(iv, il);
    assert_ne!(iv, ir);

    assert_ne!(bl, il);

    let dl = Ele::from("123".as_bytes().to_vec());
    let dr = Ele::from("123".as_bytes().to_vec());
    let dv = Ele::from("456".as_bytes().to_vec());
    assert_eq!(dl, dl);
    assert_eq!(dl, dr);
    assert_ne!(dv, dl);
    assert_ne!(dv, dr);

    assert_ne!(iv, dl);
}
