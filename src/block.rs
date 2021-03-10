use crate::consts;
use crate::errors;
use crate::hasher::Hasher;
use crate::iobuf::{Reader, Serializer, Writer};
use crate::script::*;
use crate::util;

/// 获取区块版本
fn block_version(r: u16, v: u16) -> u32 {
    (v as u32) | (r as u32) << 16
}

///区块定义
#[derive(Debug)]
pub struct Block {
    ///区块版本 (u16,u16) = (基本时间戳倍数,版本)
    ver: u32,
    ///上个区块hash
    prev: Hasher,
    ///莫克尔树id
    merkle: Hasher,
    /// 时间戳
    time: u32,
    ///区块难度
    bits: u32,
    ///随机值
    nonce: u32,
    ///交易列表
    txs: Vec<Tx>,
}

impl Default for Block {
    fn default() -> Self {
        let mut block = Block {
            ver: block_version(1, 1),
            prev: Hasher::default(),
            merkle: Hasher::default(),
            time: 0,
            bits: 0,
            nonce: 0,
            txs: vec![],
        };
        block.set_now_time();
        block
    }
}

impl Clone for Block {
    fn clone(&self) -> Self {
        Block {
            ver: self.ver,
            prev: self.prev.clone(),
            merkle: self.merkle.clone(),
            time: self.time,
            bits: self.bits,
            nonce: self.nonce,
            txs: self.txs.clone(),
        }
    }
}

impl Block {
    /// 设置当前时间
    pub fn set_now_time(&mut self) {
        let now = util::timestamp();
        self.set_timestamp(now);
    }
    /// 设置固定时间戳
    pub fn set_timestamp(&mut self, now: i64) {
        let r = now / consts::BASE_UTC_UNIX_TIME;
        let v = now - r * consts::BASE_UTC_UNIX_TIME;
        self.ver = block_version(r as u16, self.get_ver());
        self.time = v as u32;
    }
    /// 获取区块时间戳
    pub fn get_timestamp(&self) -> i64 {
        let r = ((self.ver >> 16) & 0xFFFF) as i64;
        r * consts::BASE_UTC_UNIX_TIME + self.time as i64
    }
    /// 获取版本
    pub fn get_ver(&self) -> u16 {
        (self.ver & 0xFFFF) as u16
    }
    /// 设置区块版本
    pub fn set_version(&mut self, v: u16) {
        let r = ((self.ver >> 16) & 0xFFFF) as u16;
        self.ver = block_version(r, v);
    }
    ///追加交易元素
    pub fn append(&mut self, tx: Tx) {
        self.txs.push(tx)
    }
}

#[test]
fn test_block_time() {
    let mut b = Block::default();
    b.set_timestamp(100);
    assert_eq!(b.ver, 1);
    assert_eq!(b.time, 100);

    b.set_timestamp(101);
    b.set_version(10);
    assert_eq!(b.ver, 10);
    assert_eq!(b.time, 101);

    b.set_timestamp(10 * consts::BASE_UTC_UNIX_TIME + 101);
    b.set_version(12);
    assert_eq!(b.ver, (10 << 16) | 12);
    assert_eq!(b.time, 101);

    assert_eq!(b.get_ver(), 12);
    assert_eq!(b.get_timestamp(), 10 * consts::BASE_UTC_UNIX_TIME + 101);
}

///交易
#[derive(Debug)]
pub struct Tx {
    ///交易版本
    ver: u32,
    ///输入列表
    ins: Vec<TxIn>,
    ///输出列表
    outs: Vec<TxOut>,
}

impl Serializer for Tx {
    fn encode(&self, wb: &mut Writer) {
        wb.u32(self.ver);
        wb.u16(self.ins.len() as u16);
        for inv in self.ins.iter() {
            inv.encode(wb);
        }
        wb.u16(self.outs.len() as u16);
        for out in self.outs.iter() {
            out.encode(wb);
        }
    }
    fn decode(r: &mut Reader) -> Result<Tx, errors::Error> {
        let mut v = Tx::default();
        v.ver = r.u32()?;
        for _ in 0..r.u16()? {
            let iv: TxIn = r.decode()?;
            v.ins.push(iv);
        }
        for _ in 0..r.u16()? {
            let iv: TxOut = r.decode()?;
            v.outs.push(iv);
        }
        Ok(v)
    }
}

impl Default for Tx {
    fn default() -> Self {
        Tx {
            ver: 1,
            ins: vec![],
            outs: vec![],
        }
    }
}

impl Clone for Tx {
    fn clone(&self) -> Self {
        Tx {
            ver: self.ver,
            ins: self.ins.clone(),
            outs: self.outs.clone(),
        }
    }
}

///交易输入
#[derive(Debug)]
pub struct TxIn {
    ///消费的交易id
    out: Hasher,
    ///对应的输出索引
    idx: u16,
    ///输入脚本
    script: Script,
    ///序列号
    seq: u32,
}

impl Serializer for TxIn {
    fn encode(&self, wb: &mut Writer) {
        wb.encode(&self.out);
        wb.u16(self.idx);
        wb.encode(&self.script);
        wb.u32(self.seq);
    }
    fn decode(r: &mut Reader) -> Result<TxIn, errors::Error> {
        let mut i = TxIn::default();
        i.out = r.decode()?;
        i.idx = r.u16()?;
        i.script = r.decode()?;
        i.seq = r.u32()?;
        Ok(i)
    }
}

impl Default for TxIn {
    fn default() -> Self {
        TxIn {
            out: Hasher::default(),
            idx: 0,
            script: Script::default(),
            seq: 0,
        }
    }
}

impl Clone for TxIn {
    fn clone(&self) -> Self {
        TxIn {
            out: self.out.clone(),
            idx: self.idx,
            script: self.script.clone(),
            seq: self.seq,
        }
    }
}

///交易输出
#[derive(Debug)]
pub struct TxOut {
    ///输入金额
    value: i64,
    ///输出脚本
    script: Script,
}

impl Default for TxOut {
    fn default() -> Self {
        TxOut {
            value: 0,
            script: Script::default(),
        }
    }
}

impl Clone for TxOut {
    fn clone(&self) -> Self {
        TxOut {
            script: self.script.clone(),
            value: self.value,
        }
    }
}

impl Serializer for TxOut {
    fn encode(&self, w: &mut Writer) {
        w.i64(self.value);
        w.encode(&self.script);
    }
    fn decode(r: &mut Reader) -> Result<TxOut, errors::Error> {
        let mut i = TxOut::default();
        i.value = r.i64()?;
        i.script = r.decode()?;
        Ok(i)
    }
}

#[test]
fn test_block() {
    let mut s = Script::new(32);
    s.op(crate::script::OP_00);
    s.op(crate::script::OP_01);
    s.op(crate::script::OP_02);
    let i = TxIn {
        out: Hasher::default(),
        idx: 0x12,
        script: s,
        seq: 0x34,
    };
    let mut wb = Writer::default();
    wb.encode(&i);
    let mut rb = wb.reader();
    let o: TxIn = rb.decode().unwrap();
    assert_eq!(i.out, o.out);
    assert_eq!(i.idx, o.idx);
    assert_eq!(i.script, o.script);
    assert_eq!(i.seq, o.seq);
}

#[test]
fn test_base_inout_script() {
    use crate::bytes::IntoBytes;
    //定义测试执行环境
    pub struct TestEnv {}
    impl ExectorEnv for TestEnv {
        fn get_sign_writer(&self) -> Result<Writer, errors::Error> {
            let mut w = Writer::default();
            w.put_bytes("aaa".as_bytes());
            Ok(w)
        }
    }
    let env = &TestEnv {};
    use crate::account::Account;
    let mut acc = Account::new(5, 2, false, true).unwrap();
    acc.sign_with_index(0, "aaa".as_bytes()).unwrap();
    acc.sign_with_index(1, "aaa".as_bytes()).unwrap();
    //input script
    let mut is = Script::default();
    //push account data
    //-1
    is.data(&acc.into_bytes());
    // println!("{:x?}", is);

    //out script
    let mut os = Script::default();
    //hash input script account
    //-2
    os.op(OP_HASHER);
    //push real hash data
    //-3
    os.data(&acc.hash().unwrap().into_bytes());
    //检测hash是否一致
    //-4
    os.op(OP_EQUAL_VERIFY);
    //检测签名
    //-5
    os.op(OP_CHECKSIG_VERIFY);
    // println!("{:x?}", os);
    //链接输出脚本
    is.concat(&os);

    //println!("{:x?}", is);

    let mut exector = Exector::new();
    let size = exector.exec(&is, env).unwrap();
    assert_eq!(5, size);
}
