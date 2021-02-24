use crate::bytes::{Bytes, WithBytes};
use crate::errors;
use crate::hasher::Hasher;
use crate::iobuf::{Reader, Writer};
use crate::script::Script;
///区块定义
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

impl Bytes for Block {
    fn bytes(&self) -> Vec<u8> {
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

impl WithBytes for Block {
    fn with_bytes(bb: &Vec<u8>) -> Result<Block, errors::Error> {
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
pub struct Tx {
    ///交易版本
    ver: u32,
    ///输入列表
    ins: Vec<TxIn>,
    ///输出列表
    outs: Vec<TxOut>,
}

impl Bytes for Tx {
    fn bytes(&self) -> Vec<u8> {
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

impl WithBytes for Tx {
    fn with_bytes(bb: &Vec<u8>) -> Result<Self, errors::Error> {
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

impl Bytes for TxIn {
    fn bytes(&self) -> Vec<u8> {
        let mut wb = Writer::default();
        wb.put(&self.out);
        wb.u16(self.idx);
        wb.put(&self.script);
        wb.u32(self.seq);
        return wb.bytes().to_vec();
    }
}

impl WithBytes for TxIn {
    fn with_bytes(bb: &Vec<u8>) -> Result<Self, errors::Error> {
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
pub struct TxOut {
    ///输入金额
    value: i64,
    ///输出脚本
    script: Script,
}

impl Bytes for TxOut {
    fn bytes(&self) -> Vec<u8> {
        let mut wb = Writer::default();
        wb.i64(self.value);
        wb.put(self);
        return wb.bytes().to_vec();
    }
}

impl WithBytes for TxOut {
    fn with_bytes(bb: &Vec<u8>) -> Result<Self, errors::Error> {
        let mut r = Reader::new(bb);
        Ok(TxOut {
            value: r.i64()?,
            script: r.get()?,
        })
    }
}

#[test]
fn test_block() {}
