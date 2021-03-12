use crate::account::Account;
use crate::bytes::{FromBytes, IntoBytes};
use crate::errors;
use crate::hasher::Hasher;
use crate::iobuf;
use crate::iobuf::{Reader, Serializer, Writer};
use std::convert::{From, TryFrom, TryInto};
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
/// 定义脚本类型,一般放置在脚本最前面
pub const OP_TYPE: u8 = 0xFF;
/// 验证栈顶是否为 OP_TRUE 不是就立即结束返回并丢弃栈顶的参数
pub const OP_VERIFY: u8 = 0xC0;
/// 检测数据是否相等 类型和数据都必须相等 结果放入栈顶 top(-1) == top(-2) -> push bool,丢下使用参数
pub const OP_EQUAL: u8 = 0xC1;
/// 取反true <-> false
pub const OP_NOT: u8 = 0xC2;
/// 检测签名是否正确 栈顶放入签名数据,验证成功将一个bool放入栈顶
pub const OP_CHECKSIG: u8 = 0xC3;
/// hash账户并将hash值push到堆栈,保留账户数据
pub const OP_HASHER: u8 = 0xC4;
/// equal and verify true
pub const OP_EQUAL_VERIFY: u8 = 0xC5;
/// check sig and verify true
pub const OP_CHECKSIG_VERIFY: u8 = 0xC6;
/// 检测是否是IN_OUT脚本: 输入 + 输出
pub const OP_VERIFY_INOUT: u8 = 0xC7;

/// coinbase脚本类型
pub const SCRIPT_TYPE_CB: u8 = 0x1;
/// 最大coinbase脚本长度
pub const MAX_SCRIPT_CB_SIZE: usize = 128;
/// 输入脚本
pub const SCRIPT_TYPE_IN: u8 = 0x2;
/// 最大输入脚本长度
pub const MAX_SCRIPT_IN_SIZE: usize = 2048;
/// 锁定输出脚本
pub const SCRIPT_TYPE_OUT: u8 = 0x3;
/// 最大输出脚本长度
pub const MAX_SCRIPT_OUT_SIZE: usize = 2048;
/// 脚本最大长度
pub const MAX_SCRIPT_SIZE: usize = 4096;
/// 脚本最大ops数量
pub const MAX_SCRIPT_OPS: usize = 256;

//脚本执行器
#[derive(Debug)]
pub struct Exector {
    eles: Vec<Ele>,
    typs: Vec<u8>,
}

/// 执行环境特性定义
pub trait ExectorEnv: Sized {
    /// OP_CHECKSIG OP_CHECKSIG_VERIFY 验签使用
    /// ele: 堆栈顶部元素(account数据类型)
    fn verify_sign(&self, ele: &Ele) -> Result<bool, errors::Error>;
}

impl Exector {
    //获取类型
    pub fn get_type(&self) -> &[u8] {
        &self.typs
    }
    //获取元素数量
    pub fn len(&self) -> usize {
        self.eles.len()
    }
    ///获取栈顶元素
    /// s3  -1 +3
    /// s2  -2 +2
    /// s1  -3 +1
    pub fn top(&self, n: isize) -> &Ele {
        let l = self.len() as isize;
        if n < 0 {
            &self.eles[(l + n) as usize]
        } else {
            &self.eles[(n - 1) as usize]
        }
    }
    ///
    fn pop(&mut self, n: usize) -> Result<(), errors::Error> {
        if self.eles.len() < n {
            return errors::Error::msg("StackLenErr");
        }
        self.eles.truncate(self.eles.len() - n);
        Ok(())
    }
    ///
    pub fn new() -> Exector {
        Exector {
            eles: vec![],
            typs: vec![],
        }
    }
    //检测栈元素数量至少为l个
    pub fn check(&self, l: usize) -> Result<usize, errors::Error> {
        let rl = self.len();
        if rl < l {
            errors::Error::msg("StackLenErr")
        } else {
            Ok(rl)
        }
    }
    ///执行脚本
    pub fn exec(&mut self, script: &Script, env: &impl ExectorEnv) -> Result<usize, errors::Error> {
        //脚本过大
        if script.len() > MAX_SCRIPT_SIZE {
            return errors::Error::msg("ScriptFmtErr");
        }
        let mut reader = script.reader();
        if reader.remaining() == 0 {
            return errors::Error::msg("ScriptEmptyErr");
        }
        let mut step = 0;
        loop {
            step += 1;
            let op = reader.u8()?;
            match op {
                OP_TYPE => {
                    let typ = reader.u8()?;
                    self.typs.push(typ);
                }
                OP_TRUE | OP_FALSE => {
                    self.eles.push(Ele::from(op == OP_TRUE));
                }
                OP_00..=OP_16 => {
                    self.eles.push(Ele::from(op as i64));
                }
                OP_NUMBER_1..=OP_NUMBER_8 => match op {
                    OP_NUMBER_1 => {
                        let v = reader.i8()? as i64;
                        self.eles.push(Ele::from(v));
                    }
                    OP_NUMBER_2 => {
                        let v = reader.i16()? as i64;
                        self.eles.push(Ele::from(v));
                    }
                    OP_NUMBER_4 => {
                        let v = reader.i32()? as i64;
                        self.eles.push(Ele::from(v));
                    }
                    OP_NUMBER_8 => {
                        let v = reader.i64()? as i64;
                        self.eles.push(Ele::from(v));
                    }
                    _ => return errors::Error::msg("ScriptFmtErr"),
                },
                OP_DATA_1..=OP_DATA_4 => match op {
                    OP_DATA_1 => {
                        let l = reader.u8()? as usize;
                        let d = reader.get_bytes(l)?;
                        self.eles.push(Ele::from(d));
                    }
                    OP_DATA_2 => {
                        let l = reader.u16()? as usize;
                        let d = reader.get_bytes(l)?;
                        self.eles.push(Ele::from(d));
                    }
                    OP_DATA_4 => {
                        let l = reader.u32()? as usize;
                        let d = reader.get_bytes(l)?;
                        self.eles.push(Ele::from(d));
                    }
                    _ => return errors::Error::msg("ScriptFmtErr"),
                },
                OP_VERIFY => {
                    //验证顶部元素是否为true,不是返回错误,否则删除栈顶继续
                    self.check(1)?;
                    let val: bool = self.top(-1).try_into()?;
                    self.pop(1)?;
                    if !val {
                        return errors::Error::msg("ScriptVerifyErr");
                    }
                }
                OP_HASHER => {
                    //hash top account
                    self.check(1)?;
                    let acc: Account = self.top(-1).try_into()?;
                    let hv = acc.hash()?;
                    self.eles.push(Ele::from(&hv.into_bytes()));
                }
                OP_EQUAL | OP_EQUAL_VERIFY => {
                    //比较栈顶的两个元素是否相等
                    self.check(2)?;
                    let l = self.top(-1);
                    let r = self.top(-2);
                    let val = l == r;
                    self.pop(2)?;
                    //如果只验证true不放入结果到堆栈
                    if op == OP_EQUAL_VERIFY {
                        if !val {
                            return errors::Error::msg("ScriptVerifyErr");
                        }
                    } else {
                        self.eles.push(Ele::from(val));
                    }
                }
                OP_NOT => {
                    //对栈顶的bool值取反
                    self.check(1)?;
                    let val: bool = self.top(-1).try_into()?;
                    self.pop(1)?;
                    self.eles.push(Ele::from(!val));
                }
                OP_CHECKSIG | OP_CHECKSIG_VERIFY => {
                    //检测签名,放置结果到栈顶并销毁参数数据
                    self.check(1)?;
                    let ele = self.top(-1);
                    let val = env.verify_sign(ele)?;
                    self.pop(1)?;
                    //如果只验证true不放入结果到堆栈
                    if op == OP_CHECKSIG_VERIFY {
                        if !val {
                            return errors::Error::msg("ScriptCheckSigErr");
                        }
                    } else {
                        self.eles.push(Ele::from(val));
                    }
                }
                OP_VERIFY_INOUT => {
                    //检测是否为输入+输出脚本
                    if self.typs.len() != 2 {
                        return errors::Error::msg("ScriptExeErr");
                    }
                    if self.typs != [SCRIPT_TYPE_IN, SCRIPT_TYPE_OUT] {
                        return errors::Error::msg("ScriptExeErr");
                    }
                }
                _ => {
                    return errors::Error::msg("ScriptFmtErr");
                }
            }
            if step > MAX_SCRIPT_OPS {
                return errors::Error::msg("ScriptFmtErr");
            }
            if reader.remaining() == 0 {
                break;
            }
        }
        Ok(step)
    }
}

/// 测试用特性实现
struct TestEnv;

impl ExectorEnv for TestEnv {
    fn verify_sign(&self, ele: &Ele) -> Result<bool, errors::Error> {
        let a: Account = ele.try_into()?;
        a.verify("aaa".as_bytes())
    }
}

#[test]
fn test_script_get_type() {
    let mut script = Script::new(32);
    script.set_type(SCRIPT_TYPE_CB);
    let typ = script.get_type().unwrap();
    assert_eq!(typ, SCRIPT_TYPE_CB);
}

#[test]
fn test_script_type() {
    let mut script = Script::new(32);
    script.set_type(SCRIPT_TYPE_IN);
    let mut exector = Exector::new();
    exector.exec(&script, &TestEnv {}).unwrap();
    assert_eq!(exector.len(), 0);
    assert_eq!(exector.get_type(), [SCRIPT_TYPE_IN]);
}

#[test]
fn test_op_not() {
    let mut script = Script::new(32);
    script.bool(false);
    script.op(OP_NOT);
    let mut exector = Exector::new();
    exector.exec(&script, &TestEnv {}).unwrap();
    assert_eq!(exector.len(), 1);
    let b: bool = exector.top(-1).try_into().unwrap();
    assert_eq!(b, true);

    let mut script = Script::new(32);
    script.bool(true);
    script.op(OP_NOT);
    let mut exector = Exector::new();
    exector.exec(&script, &TestEnv {}).unwrap();
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
    exector.exec(&script, &TestEnv {}).unwrap();
    assert_eq!(exector.len(), 1);
    let b: bool = exector.top(-1).try_into().unwrap();
    assert_eq!(b, true);

    let mut script = Script::new(32);
    script.op(OP_01);
    script.i16(1);
    script.op(OP_EQUAL);
    let mut exector = Exector::new();
    exector.exec(&script, &TestEnv {}).unwrap();
    assert_eq!(exector.len(), 1);
    let b: bool = exector.top(-1).try_into().unwrap();
    assert_eq!(b, true);

    let mut script = Script::new(32);
    script.string(&String::from("111"));
    script.string(&String::from("222"));
    script.op(OP_EQUAL);
    let mut exector = Exector::new();
    exector.exec(&script, &TestEnv {}).unwrap();
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
    exector.exec(&script, &TestEnv {}).unwrap();
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
    exector.exec(&script, &TestEnv {}).unwrap();
    assert_eq!(exector.len(), 0);
}

#[test]
fn test_push_data() {
    let mut script = Script::new(32);
    script.data(&[1, 2, 3]);
    script.data(&[10, 20, 30]);
    let mut exector = Exector::new();
    exector.exec(&script, &TestEnv {}).unwrap();
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
    exector.exec(&script, &TestEnv {}).unwrap();
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
    for v in OP_00..=OP_16 {
        script.op(v);
    }
    let mut exector = Exector::new();
    exector.exec(&script, &TestEnv {}).unwrap();
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
    exector.exec(&script, &TestEnv {}).unwrap();
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

#[test]
fn test_script_concat() {
    let mut script1 = Script::new(32);
    script1.op(1);
    let mut script2 = Script::new(32);
    script2.op(2);
    let mut script3 = Script::new(32);
    script3.op(3);
    script3.concat(&script2);
    script1.concat(&script3);
    assert_eq!(3, script1.len())
}

///脚本生成
#[derive(Debug)]
pub struct Script {
    writer: iobuf::Writer,
}

impl Clone for Script {
    fn clone(&self) -> Self {
        Script {
            writer: self.writer.clone(),
        }
    }
}

impl Default for Script {
    fn default() -> Self {
        Script::new(256)
    }
}

impl Script {
    ///获取脚本op数量
    pub fn ops(&self) -> Result<usize, errors::Error> {
        //脚本过大
        if self.len() > MAX_SCRIPT_SIZE {
            return errors::Error::msg("ScriptFmtErr");
        }
        let mut reader = self.reader();
        if reader.remaining() == 0 {
            return errors::Error::msg("ScriptEmptyErr");
        }
        let mut ops = 0;
        loop {
            ops += 1;
            let op = reader.u8()?;
            match op {
                OP_TYPE => {
                    reader.advance(1)?;
                }
                OP_NUMBER_1..=OP_NUMBER_8 => {
                    let p = op - OP_NUMBER_1;
                    let l = (1 << p) as usize;
                    reader.advance(l)?;
                }
                OP_DATA_1..=OP_DATA_4 => {
                    let n = (op - OP_DATA_1 + 1) as usize;
                    let l = reader.length(n)?;
                    reader.advance(l)?;
                }
                _ => {
                    //这里应该都是指令,否则需要跳过指令相关的数据
                }
            }
            if ops > MAX_SCRIPT_OPS {
                return errors::Error::msg("ScriptFmtErr");
            }
            if reader.remaining() == 0 {
                break;
            }
        }
        Ok(ops)
    }
    /// 获取脚本最大长度
    pub fn max_size(&self) -> Result<usize, errors::Error> {
        match self.get_type()? {
            SCRIPT_TYPE_CB => Ok(MAX_SCRIPT_CB_SIZE),
            SCRIPT_TYPE_IN => Ok(MAX_SCRIPT_IN_SIZE),
            SCRIPT_TYPE_OUT => Ok(MAX_SCRIPT_OUT_SIZE),
            _ => return errors::Error::msg("ScriptFmtErr"),
        }
    }
    ///检测脚本数据
    pub fn check(&self) -> Result<(), errors::Error> {
        //最大限制长度
        if self.len() > MAX_SCRIPT_SIZE {
            return errors::Error::msg("ScriptFmtErr");
        }
        //不同类型长度
        if self.len() == 0 || self.len() > self.max_size()? {
            return errors::Error::msg("ScriptFmtErr");
        }
        //执行单元数量
        if self.ops()? > MAX_SCRIPT_OPS {
            return errors::Error::msg("ScriptFmtErr");
        }
        Ok(())
    }
    //带类型创建脚本
    pub fn from(typ: u8) -> Self {
        let mut script = Self::default();
        script.set_type(typ);
        script
    }
    //获取脚本长度
    pub fn len(&self) -> usize {
        self.writer.len()
    }
    ///获取一个读取对象
    pub fn reader(&self) -> iobuf::Reader {
        self.writer.reader()
    }
    ///获取脚本内容
    pub fn bytes(&self) -> &[u8] {
        return self.writer.bytes();
    }
    /// 获取脚本类型
    pub fn get_type(&self) -> Result<u8, errors::Error> {
        let b = self.writer.bytes();
        if b.len() < 2 {
            return errors::Error::msg("ScriptFmtErr");
        }
        if b[0] != OP_TYPE {
            return errors::Error::msg("ScriptFmtErr");
        }
        Ok(b[1])
    }
    ///创建脚本对象
    pub fn new(cap: usize) -> Self {
        Script {
            writer: iobuf::Writer::new(cap),
        }
    }
    ///链接另外一个脚本
    pub fn concat(&mut self, script: &Script) -> &mut Self {
        self.writer.put_bytes(script.writer.bytes());
        return self;
    }
    ///设置脚本类型
    pub fn set_type(&mut self, typ: u8) -> &mut Self {
        self.op(OP_TYPE);
        self.writer.u8(typ);
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
        self.writer.u8(op);
        return self;
    }
    ///push string
    pub fn string(&mut self, s: &String) -> &mut Self {
        self.data(s.as_bytes());
        return self;
    }
    /// push类型
    pub fn put<T>(&mut self, v: &T)
    where
        T: IntoBytes,
    {
        self.data(&v.into_bytes());
    }
    ///push binary
    pub fn data(&mut self, v: &[u8]) -> &mut Self {
        let l = v.len();
        if l == 0 {
            return self;
        } else if l <= 0xFF {
            self.op(OP_DATA_1);
            self.writer.u8(l as u8);
        } else if l <= 0xFFFF {
            self.op(OP_DATA_2);
            self.writer.u16(l as u16);
        } else if l <= 0xFFFFFFFF {
            self.op(OP_DATA_4);
            self.writer.u32(l as u32);
        }
        self.writer.put_bytes(v);
        return self;
    }
    ///push number
    pub fn i8(&mut self, v: i8) -> &mut Self {
        self.op(OP_NUMBER_1);
        self.writer.i8(v);
        return self;
    }
    ///push number
    pub fn i16(&mut self, v: i16) -> &mut Self {
        self.op(OP_NUMBER_2);
        self.writer.i16(v);
        return self;
    }
    ///push number
    pub fn i32(&mut self, v: i32) -> &mut Self {
        self.op(OP_NUMBER_4);
        self.writer.i32(v);
        return self;
    }
    ///push number
    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.op(OP_NUMBER_4);
        self.writer.u32(v);
        return self;
    }
    ///push number
    pub fn i64(&mut self, v: i64) -> &mut Self {
        self.op(OP_NUMBER_8);
        self.writer.i64(v);
        return self;
    }
}

//a == b
impl PartialEq for Script {
    fn eq(&self, other: &Self) -> bool {
        self.writer == other.writer
    }
}

//a == a
impl Eq for Script {}

///脚本编码需要知道脚本的具体长度,所以使用put get方法
impl Serializer for Script {
    fn encode(&self, w: &mut Writer) {
        w.put(self);
    }
    fn decode(r: &mut Reader) -> Result<Script, errors::Error> {
        r.get()
    }
}

impl IntoBytes for Script {
    fn into_bytes(&self) -> Vec<u8> {
        self.writer.bytes().to_vec()
    }
}

impl FromBytes for Script {
    fn from_bytes(bb: &Vec<u8>) -> Result<Self, errors::Error> {
        match iobuf::Writer::from_bytes(bb) {
            Ok(w) => Ok(Script { writer: w }),
            Err(err) => Err(err),
        }
    }
}

///栈元素
#[derive(Debug)]
pub enum Ele {
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
        return errors::Error::msg("StackEleTypeErr");
    }
}

impl TryFrom<&Ele> for i64 {
    type Error = errors::Error;
    fn try_from(value: &Ele) -> Result<Self, Self::Error> {
        if let Ele::Number(pv) = value {
            return Ok(*pv);
        }
        return errors::Error::msg("StackEleTypeErr");
    }
}

impl<'a> TryFrom<&'a Ele> for &'a [u8] {
    type Error = errors::Error;
    fn try_from(value: &'a Ele) -> Result<Self, Self::Error> {
        if let Ele::Data(pv) = value {
            return Ok(pv);
        }
        return errors::Error::msg("StackEleTypeErr");
    }
}

impl TryFrom<&Ele> for Hasher {
    type Error = errors::Error;
    fn try_from(value: &Ele) -> Result<Self, Self::Error> {
        if let Ele::Data(pv) = value {
            return Hasher::from_bytes(pv);
        }
        return errors::Error::msg("StackEleTypeErr");
    }
}

impl TryFrom<&Ele> for Account {
    type Error = errors::Error;
    fn try_from(value: &Ele) -> Result<Self, Self::Error> {
        if let Ele::Data(pv) = value {
            return Account::from_bytes(pv);
        }
        return errors::Error::msg("StackEleTypeErr");
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
