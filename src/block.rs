use crate::account::Account;
use crate::consts;
use crate::errors::Error;
use crate::hasher::Hasher;
use crate::index::{BlkIndexer, Chain, IKey};
use crate::iobuf::{Reader, Serializer, Writer};
use crate::merkle::MerkleTree;
use crate::script::*;
use crate::store::Attr;
use crate::util;
use std::convert::TryInto;
/// 区块中最大交易数量
const MAX_TX_COUNT: u16 = 0xFFFF;

/// 数据检测特性
pub trait Checker: Sized {
    /// 检测值,收到区块或者完成区块时检测区块合法性
    /// check_value 在链入区块链的时候执行检测,在安全的线程内执行
    fn check_value(&self, _: &BlkIndexer) -> Result<(), Error>;
}

/// 最高区块描述
#[derive(Debug)]
pub struct Best {
    pub id: Hasher,  //区块id
    pub height: u32, //区块高度
}

impl Best {
    /// 是否是有效的记录
    pub fn is_valid(&self) -> bool {
        self.id != Hasher::zero() && self.height != u32::MAX
    }
    /// 获取idkey
    pub fn id_key(&self) -> IKey {
        self.id.as_ref().into()
    }
    /// 获取idkey
    pub fn height_key(&self) -> IKey {
        self.height.into()
    }
}

impl Default for Best {
    fn default() -> Self {
        Best {
            id: Hasher::zero(),
            height: u32::MAX,
        }
    }
}

impl Serializer for Best {
    fn encode(&self, wb: &mut Writer) {
        self.id.encode(wb);
        wb.u32(self.height);
    }
    fn decode(r: &mut Reader) -> Result<Best, Error> {
        let mut value = Best::default();
        value.id = r.decode()?;
        value.height = r.u32()?;
        Ok(value)
    }
}
/// 交易存储属性
#[derive(Debug)]
pub struct TxAttr {
    pub blk: Hasher, //区块id
    pub idx: u16,    //索引
}
impl Default for TxAttr {
    fn default() -> Self {
        TxAttr {
            blk: Hasher::default(),
            idx: 0,
        }
    }
}
impl Serializer for TxAttr {
    fn encode(&self, wb: &mut Writer) {
        self.blk.encode(wb);
        wb.u16(self.idx);
    }
    fn decode(r: &mut Reader) -> Result<TxAttr, Error> {
        let mut value = TxAttr::default();
        value.blk = r.decode()?;
        value.idx = r.u16()?;
        Ok(value)
    }
}

/// 区块存储属性
#[derive(Debug)]
pub struct BlkAttr {
    pub bhv: Header, //区块头
    pub hhv: u32,    //当前区块高度
    pub blk: Attr,   //数据存储位置
    pub rev: Attr,   //回退数据存储
}

impl BlkAttr {
    /// 是否包含区块数据
    pub fn has_blk(&self) -> bool {
        self.blk.is_valid()
    }
    /// 是否包含回退数据
    pub fn has_rev(&self) -> bool {
        self.rev.is_valid()
    }
}

/// 默认区块数据头
/// 区块id存储的对应数据
impl Default for BlkAttr {
    fn default() -> Self {
        BlkAttr {
            bhv: Header::default(),
            hhv: 0,
            blk: Attr::default(),
            rev: Attr::default(),
        }
    }
}

impl Serializer for BlkAttr {
    fn encode(&self, wb: &mut Writer) {
        self.bhv.encode(wb);
        wb.u32(self.hhv);
        self.blk.encode(wb);
        self.rev.encode(wb);
    }
    fn decode(r: &mut Reader) -> Result<BlkAttr, Error> {
        let mut value = BlkAttr::default();
        value.bhv = r.decode()?;
        value.hhv = r.u32()?;
        value.blk = r.decode()?;
        value.rev = r.decode()?;
        Ok(value)
    }
}

/// 区块头
#[derive(Debug)]
pub struct Header {
    /// 区块版本 (u16,u16) = (基本时间戳倍数,版本)
    /// 高16位存储了时间戳倍率,低16位存储了区块版本
    pub ver: u32,
    /// 上个区块hash
    pub prev: Hasher,
    /// 莫克尔树id
    pub merkle: Hasher,
    /// 时间戳
    pub time: u32,
    /// 区块难度
    pub bits: u32,
    /// 随机值
    pub nonce: u32,
}

impl Header {
    /// 合并区块时间戳倍率和区块版本到一个整数
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
    pub fn set_ver(&mut self, v: u16) {
        let r = ((self.ver >> 16) & 0xFFFF) as u16;
        self.ver = Self::block_version(r, v);
    }
}

impl PartialEq for Header {
    fn eq(&self, other: &Header) -> bool {
        self.ver == other.ver
            && self.prev == other.prev
            && self.merkle == other.merkle
            && self.time == other.time
            && self.bits == other.bits
            && self.nonce == other.nonce
    }
}

impl Checker for Header {
    fn check_value(&self, _: &BlkIndexer) -> Result<(), Error> {
        //检测时间戳是否正确
        if self.get_timestamp() < util::timestamp() {
            return Error::msg("block timestamp error");
        }
        //检测默克尔树id是否填充
        if self.merkle == Hasher::zero() {
            return Error::msg("merkle not set");
        }
        Ok(())
    }
}

impl Default for Header {
    fn default() -> Self {
        Header {
            ver: Self::block_version(1, 1),
            prev: Hasher::default(),
            merkle: Hasher::default(),
            time: 0,
            bits: 0,
            nonce: 0,
        }
    }
}

impl Clone for Header {
    fn clone(&self) -> Self {
        Header {
            ver: self.ver,
            prev: self.prev.clone(),
            merkle: self.merkle.clone(),
            time: self.time,
            bits: self.bits,
            nonce: self.nonce,
        }
    }
}

impl Serializer for Header {
    fn encode(&self, wb: &mut Writer) {
        wb.u32(self.ver);
        wb.encode(&self.prev);
        wb.encode(&self.merkle);
        wb.u32(self.time);
        wb.u32(self.bits);
        wb.u32(self.nonce);
    }
    fn decode(r: &mut Reader) -> Result<Header, Error> {
        let mut header = Header::default();
        header.ver = r.u32()?;
        header.prev = r.decode()?;
        header.merkle = r.decode()?;
        header.time = r.u32()?;
        header.bits = r.u32()?;
        header.nonce = r.u32()?;
        Ok(header)
    }
}

///区块定义
#[derive(Debug)]
pub struct Block {
    //区块头
    pub header: Header,
    ///交易列表
    pub txs: Vec<Tx>,
    finish: bool,
}

impl PartialEq for Block {
    fn eq(&self, other: &Block) -> bool {
        self.header == other.header && self.txs == other.txs
    }
}

impl Checker for Block {
    fn check_value(&self, ctx: &BlkIndexer) -> Result<(), Error> {
        let conf = ctx.config();
        //检测区块头
        self.header.check_value(ctx)?;
        //检测区块区块交易
        for iv in self.txs.iter() {
            iv.check_value(ctx)?
        }
        //检测金额是否正确 输入 > 输出
        //检测工作难度是否达到设置的要求
        let id = self.id()?;
        if !id.verify_pow(&conf.pow_limit, self.header.bits) {
            return Error::msg("block bits error");
        }
        Ok(())
    }
}

impl Default for Block {
    fn default() -> Self {
        Block {
            header: Header::default(),
            txs: vec![],
            finish: false,
        }
    }
}

impl Clone for Block {
    fn clone(&self) -> Self {
        Block {
            header: self.header.clone(),
            txs: self.txs.clone(),
            finish: self.finish,
        }
    }
}

impl Serializer for Block {
    fn encode(&self, wb: &mut Writer) {
        //
        self.header.encode(wb);
        //交易数量
        wb.u16(self.txs.len() as u16);
        for tx in self.txs.iter() {
            tx.encode(wb);
        }
    }
    fn decode(r: &mut Reader) -> Result<Block, Error> {
        let mut blk = Block::default();
        //
        blk.header = r.decode()?;
        //读取交易数量
        for _ in 0..r.u16()? {
            let tx: Tx = r.decode()?;
            blk.append(tx);
        }
        Ok(blk)
    }
}

impl Block {
    /// 区块是否调用完成
    pub fn is_finish(&self) -> bool {
        self.finish
    }
    /// 检测连入区块前调用
    pub fn finish(&mut self) -> Result<(), Error> {
        self.header.merkle = self.compute_merkle()?;
        self.finish = true;
        Ok(())
    }
    /// 按索引获取交易
    pub fn get_tx(&self, idx: usize) -> Result<&Tx, Error> {
        if idx >= self.txs.len() {
            return Error::msg("NotFoundTx");
        }
        Ok(&self.txs[idx])
    }
    /// 计算区块id
    pub fn id(&self) -> Result<Hasher, Error> {
        let mut wb = Writer::default();
        self.header.encode(&mut wb);
        Ok(Hasher::hash(&wb.bytes()))
    }
    /// 获取区块数据
    pub fn bytes(&self) -> Writer {
        let mut wb = Writer::default();
        self.encode(&mut wb);
        wb
    }
    /// 计算merkle值
    pub fn compute_merkle(&self) -> Result<Hasher, Error> {
        let mut ids = vec![];
        for iv in self.txs.iter() {
            ids.push(iv.id()?);
        }
        MerkleTree::compute(&ids)
    }
    ///追加交易元素
    pub fn append(&mut self, tx: Tx) {
        self.txs.push(tx)
    }
}

#[test]
fn test_block_time() {
    let mut b = Block::default();
    b.header.set_timestamp(100);
    assert_eq!(b.header.ver, 1);
    assert_eq!(b.header.time, 100);

    b.header.set_timestamp(101);
    b.header.set_ver(10);
    assert_eq!(b.header.ver, 10);
    assert_eq!(b.header.time, 101);

    b.header
        .set_timestamp(10 * consts::BASE_UTC_UNIX_TIME + 101);
    b.header.set_ver(12);
    assert_eq!(b.header.ver, (10 << 16) | 12);
    assert_eq!(b.header.time, 101);

    assert_eq!(b.header.get_ver(), 12);
    assert_eq!(
        b.header.get_timestamp(),
        10 * consts::BASE_UTC_UNIX_TIME + 101
    );
}

impl Script {
    /// 为计算id和签名写入相关数据
    pub fn encode_sign(&self, wb: &mut Writer) -> Result<(), Error> {
        match self.get_type()? {
            SCRIPT_TYPE_CB | SCRIPT_TYPE_OUT => wb.put_bytes(self.bytes()),
            SCRIPT_TYPE_IN => {
                //写入op类型
                wb.put_bytes(&[OP_TYPE, SCRIPT_TYPE_IN]);
                //为输入脚本获取数据创建的环境,环境方法应该不会执行
                struct InEnv {}
                impl ExectorEnv for InEnv {
                    fn verify_sign(&self, _: &Ele) -> Result<bool, Error> {
                        panic!("not run here!!!");
                    }
                }
                //输入脚本签名时不包含签名信息
                let mut exector = Exector::new();
                let ops = exector.exec(self, &InEnv {})?;
                if ops > MAX_SCRIPT_OPS {
                    return Error::msg("Script opts > MAX_SCRIPT_OPS");
                }
                exector.check(1)?;
                let ele = exector.top(-1);
                let acc: Account = ele.try_into()?;
                //为签名编码账户数据
                acc.encode_sign(wb)?;
            }
            _ => return Error::msg("ScriptFmtErr"),
        }
        Ok(())
    }
    /// 区块高度
    /// 自定义数据
    pub fn new_script_cb(height: u32, data: &[u8]) -> Result<Self, Error> {
        let mut script = Script::from(SCRIPT_TYPE_CB);
        script.u32(height);
        script.data(data);
        script.check()?;
        Ok(script)
    }
    /// 根据账号创建标准输入脚本
    pub fn new_script_in(acc: &Account) -> Result<Self, Error> {
        let mut script = Script::from(SCRIPT_TYPE_IN);
        script.put(acc);
        script.check()?;
        Ok(script)
    }
    /// 根据账户hash创建标准输出脚本
    pub fn new_script_out(hasher: &Hasher) -> Result<Self, Error> {
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
    pub ver: u32,
    ///输入列表
    pub ins: Vec<TxIn>,
    ///输出列表
    pub outs: Vec<TxOut>,
}

impl Checker for Tx {
    fn check_value(&self, ctx: &BlkIndexer) -> Result<(), Error> {
        //coinbase只能有一个输入
        if self.is_coinbase() && self.ins.len() != 1 {
            return Error::msg("InvalidTx");
        }
        //输出不能为空
        if self.outs.len() == 0 {
            return Error::msg("InvalidTx");
        }
        for iv in self.ins.iter() {
            iv.check_value(ctx)?;
        }
        for iv in self.outs.iter() {
            iv.check_value(ctx)?
        }
        Ok(())
    }
}

impl Tx {
    /// 检查输入输出金额
    fn check_amount(&self) -> Result<(), Error> {
        Error::msg("not finish")
    }
    /// 检测交易是否是coinbase交易
    /// 只有一个输入,并且out不指向任何一个hash
    pub fn is_coinbase(&self) -> bool {
        self.ins.len() > 0 && self.ins[0].is_coinbase()
    }
    /// 编码签名数据
    fn encode_sign(&self, wb: &mut Writer) -> Result<(), Error> {
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
    pub fn id(&self) -> Result<Hasher, Error> {
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
    fn decode(r: &mut Reader) -> Result<Tx, Error> {
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
    pub out: Hasher,
    ///对应的输出索引
    pub idx: u16,
    ///输入脚本
    pub script: Script,
    ///序列号
    pub seq: u32,
}

impl Checker for TxIn {
    fn check_value(&self, _ctx: &BlkIndexer) -> Result<(), Error> {
        //检测脚本类型,输入必须输入或者cb脚本
        let typ = self.script.get_type()?;
        if self.is_coinbase() {
            if typ != SCRIPT_TYPE_CB {
                return Error::msg("ScriptFmtErr");
            }
        } else if typ != SCRIPT_TYPE_IN {
            return Error::msg("ScriptFmtErr");
        }
        self.script.check()
    }
}

impl TxIn {
    /// 获取引用的交易输出
    pub fn get_tx_out(&self, ctx: &Chain) -> Result<TxOut, Error> {
        if self.is_coinbase() {
            return Error::msg("coinbase not exists txout");
        }
        let tx = ctx.get_tx(&self.out.as_ref().into())?;
        if self.idx >= tx.outs.len() as u16 {
            return Error::msg("idx outbound tx outs len");
        }
        Ok(tx.outs[self.idx as usize].clone())
    }
    /// 获取输入金额
    pub fn get_coin(&self, ctx: &Chain) -> Result<i64, Error> {
        //cb 没有引用的输出
        if self.is_coinbase() {
            return Ok(0);
        }
        Ok(self.get_tx_out(ctx)?.value)
    }
    /// 是否是coinbase输入
    pub fn is_coinbase(&self) -> bool {
        self.out == Hasher::zero() && self.idx == 0
    }
    /// 获取需要签名的数据
    fn encode_sign(&self, wb: &mut Writer) -> Result<(), Error> {
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
    fn decode(r: &mut Reader) -> Result<TxIn, Error> {
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
    pub value: i64,
    ///输出脚本
    pub script: Script,
}

impl Checker for TxOut {
    fn check_value(&self, _ctx: &BlkIndexer) -> Result<(), Error> {
        //检测金额
        if !consts::is_valid_amount(self.value) {
            return Error::msg("InvalidAmount");
        }
        //检测脚本类型
        if self.script.get_type()? != SCRIPT_TYPE_OUT {
            return Error::msg("ScriptFmtErr");
        }
        //检测脚本
        self.script.check()
    }
}

impl TxOut {
    fn encode_sign(&self, wb: &mut Writer) -> Result<(), Error> {
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
    fn decode(r: &mut Reader) -> Result<TxOut, Error> {
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
        fn verify_sign(&self, ele: &Ele) -> Result<bool, Error> {
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
