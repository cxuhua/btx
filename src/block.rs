use crate::account::Account;
use crate::consts;
use crate::errors;
use crate::hasher::Hasher;
use crate::iobuf::{Reader, Serializer, Writer};
use crate::merkle::MerkleTree;
use crate::script::*;
use crate::util;
use std::convert::TryInto;

/// 区块中最大交易数量
const MAX_TX_COUNT: u16 = 0xFFFF;

/// 数据检测特性
pub trait Checker: Sized {
    /// 检测值,收到区块或者完成区块时检测区块合法性
    fn check_value(&self) -> Result<(), errors::Error>;
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

impl Checker for Block {
    fn check_value(&self) -> Result<(), errors::Error> {
        for iv in self.txs.iter() {
            iv.check_value()?
        }
        Ok(())
    }
}

impl Default for Block {
    fn default() -> Self {
        let mut block = Block {
            ver: Self::block_version(1, 1),
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

impl Serializer for Block {
    fn encode(&self, wb: &mut Writer) {
        self.encode_header(wb);
        //交易数量
        wb.u16(self.txs.len() as u16);
        for tx in self.txs.iter() {
            tx.encode(wb);
        }
    }
    fn decode(r: &mut Reader) -> Result<Block, errors::Error> {
        let mut blk = Block::default();
        blk.decode_header(r)?;
        //读取交易数量
        for _ in 0..r.u16()? {
            let tx: Tx = r.decode()?;
            blk.txs.push(tx);
        }
        Ok(blk)
    }
}

impl Block {
    /// 按索引获取交易
    pub fn get_tx(&self, idx: usize) -> Result<&Tx, errors::Error> {
        if idx >= self.txs.len() {
            return errors::Error::msg("NotFoundTx");
        }
        Ok(&self.txs[idx])
    }
    /// 解码头部
    fn decode_header(&mut self, r: &mut Reader) -> Result<(), errors::Error> {
        self.ver = r.u32()?;
        self.prev = r.decode()?;
        self.merkle = r.decode()?;
        self.time = r.u32()?;
        self.bits = r.u32()?;
        self.nonce = r.u32()?;
        Ok(())
    }
    /// 编码区块头部
    fn encode_header(&self, wb: &mut Writer) {
        wb.u32(self.ver);
        wb.encode(&self.prev);
        wb.encode(&self.merkle);
        wb.u32(self.time);
        wb.u32(self.bits);
        wb.u32(self.nonce);
    }
    /// 计算区块id
    pub fn id(&self) -> Result<Hasher, errors::Error> {
        let mut wb = Writer::default();
        self.encode_header(&mut wb);
        Ok(Hasher::hash(&wb.bytes()))
    }
    /// 计算merkle值
    pub fn compute_merkle(&self) -> Result<Hasher, errors::Error> {
        let mut ids = vec![];
        for iv in self.txs.iter() {
            let id = iv.id()?;
            ids.push(id);
        }
        let root = MerkleTree::compute(&ids)?;
        Ok(root)
    }
    /// 获取区块版本
    fn block_version(r: u16, v: u16) -> u32 {
        (v as u32) | (r as u32) << 16
    }
    /// 设置当前时间
    pub fn set_now_time(&mut self) {
        let now = util::timestamp();
        self.set_timestamp(now);
    }
    /// 设置固定时间戳
    pub fn set_timestamp(&mut self, now: i64) {
        let r = now / consts::BASE_UTC_UNIX_TIME;
        let v = now - r * consts::BASE_UTC_UNIX_TIME;
        self.ver = Self::block_version(r as u16, self.get_ver());
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
        self.ver = Self::block_version(r, v);
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

impl Script {
    /// 为计算id和签名写入相关数据
    pub fn encode_sign(&self, wb: &mut Writer) -> Result<(), errors::Error> {
        match self.get_type()? {
            SCRIPT_TYPE_CB | SCRIPT_TYPE_OUT => wb.put_bytes(self.bytes()),
            SCRIPT_TYPE_IN => {
                //写入op类型
                wb.put_bytes(&[OP_TYPE, SCRIPT_TYPE_IN]);
                //为输入脚本获取数据创建的环境,环境方法应该不会执行
                struct InEnv {}
                impl ExectorEnv for InEnv {
                    fn verify_sign(&self, _: &Ele) -> Result<bool, errors::Error> {
                        panic!("not run here!!!");
                    }
                }
                //输入脚本签名时不包含签名信息
                let mut exector = Exector::new();
                let ops = exector.exec(self, &InEnv {})?;
                if ops > MAX_SCRIPT_OPS {
                    return errors::Error::msg("ScriptFmtErr");
                }
                exector.check(1)?;
                let ele = exector.top(-1);
                let acc: Account = ele.try_into()?;
                //为签名编码账户数据
                acc.encode_sign(wb)?;
            }
            _ => return errors::Error::msg("ScriptFmtErr"),
        }
        Ok(())
    }
    /// 区块高度
    /// 自定义数据
    pub fn new_script_cb(height: u32, data: &[u8]) -> Result<Self, errors::Error> {
        let mut script = Script::from(SCRIPT_TYPE_CB);
        script.u32(height);
        script.data(data);
        script.check()?;
        Ok(script)
    }
    /// 根据账号创建标准输入脚本
    pub fn new_script_in(acc: &Account) -> Result<Self, errors::Error> {
        let mut script = Script::from(SCRIPT_TYPE_IN);
        script.put(acc);
        script.check()?;
        Ok(script)
    }
    /// 根据账户hash创建标准输出脚本
    pub fn new_script_out(hasher: &Hasher) -> Result<Self, errors::Error> {
        let mut script = Script::from(SCRIPT_TYPE_OUT);
        script.op(OP_VERIFY_INOUT);
        script.op(OP_HASHER);
        script.put(hasher);
        script.op(OP_EQUAL_VERIFY);
        script.op(OP_CHECKSIG_VERIFY);
        script.check()?;
        Ok(script)
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

impl Checker for Tx {
    fn check_value(&self) -> Result<(), errors::Error> {
        //coinbase只能有一个输入
        if self.is_coinbase() && self.ins.len() != 1 {
            return errors::Error::msg("InvalidTx");
        }
        //输出不能为空
        if self.outs.len() == 0 {
            return errors::Error::msg("InvalidTx");
        }
        for iv in self.ins.iter() {
            iv.check_value()?;
        }
        for iv in self.outs.iter() {
            iv.check_value()?
        }
        Ok(())
    }
}

impl Tx {
    /// 检测交易是否是coinbase交易
    /// 只有一个输入,并且out不指向任何一个hash
    pub fn is_coinbase(&self) -> bool {
        self.ins.len() > 0 && self.ins[0].is_coinbase()
    }
    /// 编码签名数据
    fn encode_sign(&self, wb: &mut Writer) -> Result<(), errors::Error> {
        wb.u32(self.ver);
        wb.u16(self.ins.len() as u16);
        for inv in self.ins.iter() {
            inv.encode_sign(wb)?;
        }
        wb.u16(self.outs.len() as u16);
        for out in self.outs.iter() {
            out.encode_sign(wb)?;
        }
        Ok(())
    }
    // 获取交易id
    pub fn id(&self) -> Result<Hasher, errors::Error> {
        let mut wb = Writer::default();
        self.encode_sign(&mut wb)?;
        Ok(Hasher::hash(wb.bytes()))
    }
}

#[test]
fn test_tx_serializer() {
    let acc = Account::new(5, 2, false, true).unwrap();
    let mut tx = Tx::default();
    tx.ver = 1;
    let mut inv = TxIn::default();
    inv.out = Hasher::hash(&[1]);
    inv.idx = 0;
    inv.script = Script::new_script_in(&acc).unwrap();
    inv.seq = 0x11223344;
    tx.ins.push(inv);
    let mut out = TxOut::default();
    out.value = 0x6789;
    out.script = Script::new_script_out(&acc.hash().unwrap()).unwrap();
    tx.outs.push(out);

    let mut wb = Writer::default();
    wb.encode(&tx);

    let mut reader = wb.reader();
    let tx2: Tx = reader.decode().unwrap();

    assert_eq!(tx, tx2);
    assert_eq!(tx.id().unwrap(), tx2.id().unwrap());
}

#[test]
fn test_tx_hash() {
    let acc = Account::new(5, 2, false, true).unwrap();
    let mut tx = Tx::default();
    tx.ver = 1;
    let mut inv = TxIn::default();
    inv.out = Hasher::hash(&[1]);
    inv.idx = 0;
    inv.script = Script::new_script_in(&acc).unwrap();
    inv.seq = 0x11223344;
    tx.ins.push(inv);
    let mut out = TxOut::default();
    out.value = 0x6789;
    out.script = Script::new_script_out(&acc.hash().unwrap()).unwrap();
    tx.outs.push(out);
    tx.id().unwrap();
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

impl PartialEq for Tx {
    fn eq(&self, other: &Self) -> bool {
        self.ver == other.ver && self.ins == other.ins && self.outs == other.outs
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

impl Checker for TxIn {
    fn check_value(&self) -> Result<(), errors::Error> {
        //检测脚本类型,输入必须输入或者cb脚本
        let typ = self.script.get_type()?;
        if self.is_coinbase() {
            if typ != SCRIPT_TYPE_CB {
                return errors::Error::msg("ScriptFmtErr");
            }
        } else if typ != SCRIPT_TYPE_IN {
            return errors::Error::msg("ScriptFmtErr");
        }
        self.script.check()
    }
}

impl TxIn {
    /// 是否是coinbase输入
    pub fn is_coinbase(&self) -> bool {
        self.out == Hasher::zero() && self.idx == 0
    }
    fn encode_sign(&self, wb: &mut Writer) -> Result<(), errors::Error> {
        wb.encode(&self.out);
        wb.u16(self.idx);
        self.script.encode_sign(wb)?;
        wb.u32(self.seq);
        Ok(())
    }
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

impl PartialEq for TxIn {
    fn eq(&self, other: &Self) -> bool {
        self.out == other.out
            && self.script == other.script
            && self.idx == other.idx
            && self.seq == other.seq
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

impl Checker for TxOut {
    fn check_value(&self) -> Result<(), errors::Error> {
        //检测金额
        if !consts::is_valid_amount(&self.value) {
            return errors::Error::msg("InvalidAmount");
        }
        //检测脚本类型
        if self.script.get_type()? != SCRIPT_TYPE_OUT {
            return errors::Error::msg("ScriptFmtErr");
        }
        //检测脚本
        self.script.check()
    }
}

impl TxOut {
    fn encode_sign(&self, wb: &mut Writer) -> Result<(), errors::Error> {
        wb.i64(self.value);
        self.script.encode_sign(wb)?;
        Ok(())
    }
}

impl PartialEq for TxOut {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value && self.script == other.script
    }
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
    use crate::script::Ele;
    //定义测试执行环境
    pub struct TestEnv {}
    impl ExectorEnv for TestEnv {
        fn verify_sign(&self, ele: &Ele) -> Result<bool, errors::Error> {
            let acc: Account = ele.try_into()?;
            acc.verify("aaa".as_bytes())
        }
    }
    let env = &TestEnv {};
    let mut acc = Account::new(5, 2, false, true).unwrap();
    acc.sign_with_index(0, "aaa".as_bytes()).unwrap();
    acc.sign_with_index(1, "aaa".as_bytes()).unwrap();
    //创建输入脚本
    //-1 设置脚本类型
    let mut is = Script::new_script_in(&acc).unwrap();
    assert_eq!(SCRIPT_TYPE_IN, is.get_type().unwrap());

    let os = Script::new_script_out(&acc.hash().unwrap()).unwrap();
    assert_eq!(SCRIPT_TYPE_OUT, os.get_type().unwrap());

    is.concat(&os);

    let ops = is.ops().unwrap();
    assert_eq!(8, ops);

    let mut exector = Exector::new();
    let size = exector.exec(&is, env).unwrap();
    assert_eq!(ops, size);
}
