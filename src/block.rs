use crate::bytes::{FromBytes, IntoBytes};
use crate::errors;
use crate::hasher::Hasher;
use crate::iobuf::{Reader, Serializer, Writer};
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
    pub fn append(&mut self, tx: Tx) {
        self.txs.push(tx)
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
    println!("{:?}", o);
}
