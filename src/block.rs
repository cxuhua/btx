use crate::bytes::{IntoBytes, FromBytes};
use crate::errors;
use crate::hasher::Hasher;
use crate::iobuf::{Reader, Writer};
use crate::script::Script;
///区块定义
#[derive(Debug)]
pub struct Block {
    ///区块版本
    ver: u32,
    ///上个区块hash
    prev: Hasher,
    ///莫克尔树id
    merkle: Hasher,
    ///时间戳
    time: u32,
    ///区块难度
    bits: u32,
    ///随机值
    nonce: u32,
    ///交易列表
    txs: Vec<Tx>,
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
    ///追加交易元素
    fn append(&mut self, tx: Tx) {
        self.txs.push(tx)
    }
}

impl Default for Block {
    fn default() -> Self {
        Block {
            ver: 1,
            prev: Hasher::default(),
            merkle: Hasher::default(),
            time: 0,
            bits: 0,
            nonce: 0,
            txs: vec![],
        }
    }
}

impl IntoBytes for Block {
    fn into_bytes(&self) -> Vec<u8> {
        let mut wb = Writer::default();
        wb.u32(self.ver);
        wb.put(&self.prev);
        wb.put(&self.merkle);
        wb.u32(self.time);
        wb.u32(self.bits);
        wb.u32(self.nonce);
        wb.u16(self.txs.len() as u16);
        for v in self.txs.iter() {
            wb.put(v);
        }
        return wb.bytes().to_vec();
    }
}

impl FromBytes for Block {
    fn from_bytes(bb: &Vec<u8>) -> Result<Block, errors::Error> {
        let mut r = Reader::new(bb);
        let mut v = Block::default();
        v.ver = r.u32()?;
        v.prev = r.get()?;
        v.merkle = r.get()?;
        v.time = r.u32()?;
        v.bits = r.u32()?;
        v.nonce = r.u32()?;
        for _ in 0..r.u16()? {
            let iv: Tx = r.get()?;
            v.txs.push(iv);
        }
        Ok(v)
    }
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

impl Clone for Tx {
    fn clone(&self) -> Self {
        Tx {
            ver: self.ver,
            ins: self.ins.clone(),
            outs: self.outs.clone(),
        }
    }
}

impl IntoBytes for Tx {
    fn into_bytes(&self) -> Vec<u8> {
        let mut wb = Writer::default();
        wb.u32(self.ver);
        wb.u16(self.ins.len() as u16);
        for inv in self.ins.iter() {
            wb.put(inv);
        }
        wb.u16(self.outs.len() as u16);
        for out in self.outs.iter() {
            wb.put(out);
        }
        return wb.bytes().to_vec();
    }
}

impl FromBytes for Tx {
    fn from_bytes(bb: &Vec<u8>) -> Result<Self, errors::Error> {
        let mut r = Reader::new(bb);
        let mut v = Tx {
            ver: 0,
            ins: vec![],
            outs: vec![],
        };
        v.ver = r.u32()?;
        for _ in 0..r.u16()? {
            let iv: TxIn = r.get()?;
            v.ins.push(iv);
        }
        for _ in 0..r.u16()? {
            let iv: TxOut = r.get()?;
            v.outs.push(iv);
        }
        Ok(v)
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

impl IntoBytes for TxIn {
    fn into_bytes(&self) -> Vec<u8> {
        let mut wb = Writer::default();
        wb.put(&self.out);
        wb.u16(self.idx);
        wb.put(&self.script);
        wb.u32(self.seq);
        return wb.bytes().to_vec();
    }
}

impl FromBytes for TxIn {
    fn from_bytes(bb: &Vec<u8>) -> Result<Self, errors::Error> {
        let mut r = Reader::new(bb);
        Ok(TxIn {
            out: r.get()?,
            idx: r.u16()?,
            script: r.get()?,
            seq: r.u32()?,
        })
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

impl Clone for TxOut {
    fn clone(&self) -> Self {
        TxOut {
            script: self.script.clone(),
            value: self.value,
        }
    }
}

impl IntoBytes for TxOut {
    fn into_bytes(&self) -> Vec<u8> {
        let mut wb = Writer::default();
        wb.i64(self.value);
        wb.put(self);
        return wb.bytes().to_vec();
    }
}

impl FromBytes for TxOut {
    fn from_bytes(bb: &Vec<u8>) -> Result<Self, errors::Error> {
        let mut r = Reader::new(bb);
        Ok(TxOut {
            value: r.i64()?,
            script: r.get()?,
        })
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
        idx: 0,
        script: s,
        seq: 0,
    };
    let mut wb = Writer::default();
    wb.put(&i);
    let mut rb = wb.reader();
    let o: TxIn = rb.get().unwrap();
    println!("{:?}", o);
}
