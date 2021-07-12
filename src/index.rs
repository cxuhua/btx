use crate::account::{Account, HasAddress};
use crate::block::{Best, BlkAttr, Block, Checker, Tx, TxAttr, TxIn, TxOut};
use crate::bytes::IntoBytes;
use crate::config::Config;
use crate::consts;
use crate::errors::Error;
use crate::hasher::Hasher;
use crate::iobuf::Writer;
use crate::iobuf::{Reader, Serializer};
use crate::leveldb::{IBatch, LevelDB};
use crate::script::{Ele, Exector, ExectorEnv};
use crate::store::Store;
use crate::util;
use bytes::BufMut;
use core::hash;
use db_key::Key;
use lru::LruCache;
use std::cmp::{Eq, PartialEq};
use std::convert::{Into, TryInto};
use std::path::Path;
use std::sync::Arc;
use std::sync::RwLock;

#[derive(PartialEq, Eq, Clone, PartialOrd, Ord, Debug)]
pub struct IKey(Vec<u8>);

/// 自身包含key的兑现无需指定写入key
pub trait HasKey {
    //返回对象的key
    fn key(&self) -> IKey;
}

///
impl Key for IKey {
    fn from_u8(key: &[u8]) -> IKey {
        IKey(key.to_vec())
    }
    fn as_slice<T, F: Fn(&[u8]) -> T>(&self, f: F) -> T {
        f(self.bytes())
    }
}

impl From<u32> for IKey {
    fn from(v: u32) -> Self {
        let mut key = IKey(vec![]);
        key.0.put_u32_le(v);
        key
    }
}

impl From<&[u8]> for IKey {
    fn from(v: &[u8]) -> Self {
        IKey(v.to_vec())
    }
}

impl From<&str> for IKey {
    fn from(v: &str) -> Self {
        IKey(v.as_bytes().to_vec())
    }
}

///
impl From<Vec<u8>> for IKey {
    fn from(v: Vec<u8>) -> Self {
        IKey(v)
    }
}

/// 按hash查询
impl From<&Hasher> for IKey {
    fn from(v: &Hasher) -> Self {
        IKey(v.into_bytes())
    }
}

impl hash::Hash for IKey {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        state.write(&self.0);
    }
}

impl IKey {
    //是否是高度key
    pub fn is_height_key(&self) -> bool {
        self.len() == 4
    }
    /// key 长度
    pub fn len(&self) -> usize {
        self.0.len()
    }
    /// 获取key字节
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
    /// 连接字节到key
    pub fn concat(&mut self, v: &[u8]) -> &mut Self {
        self.0.put_slice(v);
        self
    }
    /// 空key
    pub fn empty() -> Self {
        "".as_bytes().into()
    }
    /// 是否包含前缀
    pub fn starts_with(&self, prefix: &IKey) -> bool {
        self.0.starts_with(&prefix.0)
    }
}

/// 金额数据结构
/// 地址对应的金额将存储在key方法返回的key中
/// cpk tx idx 使用存储key去填充
#[derive(Clone, Debug)]
pub struct CoinAttr {
    cpk: Hasher, //拥有此金额的公钥hasher
    tx: Hasher,  //所在交易
    idx: u16,    //所在交易的输出
    value: i64,  //金额
    base: u8,    //是否在coinbase交易
    height: u32, //所在区块高度
}

impl HasAddress for CoinAttr {
    //cpk就是地址hasher直接返回
    fn get_address(&self) -> Result<Hasher, Error> {
        Ok(self.cpk.clone())
    }
}

impl Serializer for CoinAttr {
    /// 编码数据到writer
    fn encode(&self, w: &mut Writer) {
        w.i64(self.value);
        w.u8(self.base);
        w.u32(self.height);
    }
    /// 从reader读取数据
    fn decode(r: &mut Reader) -> Result<Self, Error>
    where
        Self: Default,
    {
        let mut coin = Self::default();
        coin.value = r.i64()?;
        coin.base = r.u8()?;
        coin.height = r.u32()?;
        Ok(coin)
    }
}

impl Default for CoinAttr {
    fn default() -> Self {
        CoinAttr {
            cpk: Hasher::zero(),
            tx: Hasher::zero(),
            idx: 0,
            value: 0,
            base: 0,
            height: 0,
        }
    }
}

/// coin可以作为兑现直接按次key存储
impl HasKey for CoinAttr {
    /// 获取存储key
    fn key(&self) -> IKey {
        let mut w = Writer::default();
        w.encode(&self.cpk);
        w.encode(&self.tx);
        w.u16(self.idx);
        IKey::from_u8(w.bytes())
    }
}

impl CoinAttr {
    /// 金额在spent高度上是否可用
    pub fn is_matured(&self, spent: u32) -> bool {
        //非coinbase可用
        if self.base == 0 {
            return true;
        }
        //coinbase输出必须在100个高度后才可消费
        return spent - self.height >= consts::COINBASE_MATURITY;
    }
    /// 从存储key获取
    pub fn from_key(k: &IKey) -> Result<Self, Error> {
        let mut r = Reader::new(k.bytes());
        let mut v = Self::default();
        v.cpk = r.decode()?;
        v.tx = r.decode()?;
        v.idx = r.u16()?;
        Ok(v)
    }
    /// 从存储key获取
    pub fn fill_key(&mut self, k: &IKey) -> Result<&Self, Error> {
        let mut r = Reader::new(k.bytes());
        self.cpk = r.decode()?;
        self.tx = r.decode()?;
        self.idx = r.u16()?;
        Ok(self)
    }
    /// 填充值
    pub fn fill_value_with_reader(&mut self, r: &mut Reader) -> Result<&mut Self, Error> {
        self.value = r.i64()?;
        self.base = r.u8()?;
        self.height = r.u32()?;
        Ok(self)
    }
    /// 填充值
    pub fn fill_value(&mut self, bytes: &[u8]) -> Result<&mut Self, Error> {
        self.fill_value_with_reader(&mut Reader::new(bytes))
    }
    /// 获取值
    pub fn value(&self) -> Writer {
        let mut w = Writer::default();
        self.encode(&mut w);
        w
    }
}

#[test]
fn test_coin_attr_key_value() {
    use std::convert::TryFrom;
    let mut attr = CoinAttr::default();
    attr.cpk = Hasher::try_from("12345678ffffffffffffffffffffffffffffffffffffffffffffffff12345678")
        .unwrap();
    attr.tx = Hasher::try_from("87654321ffffffffffffffffffffffffffffffffffffffffffffffff87654321")
        .unwrap();
    attr.idx = 0x9988;
    attr.value = 78965;
    attr.base = 1;
    attr.height = 2678;
    let value = attr.value();
    let mut attr2 = CoinAttr::from_key(&attr.key()).unwrap();
    let attr2 = attr2.fill_value(value.bytes()).unwrap();
    assert_eq!(attr.cpk, attr2.cpk);
    assert_eq!(attr.tx, attr2.tx);
    assert_eq!(attr.idx, attr2.idx);
    assert_eq!(attr.value, attr2.value);
    assert_eq!(attr.base, attr2.base);
    assert_eq!(attr.height, attr2.height);
}

#[test]
fn test_block_index_coin_save() {
    Config::test(|conf, idx| {
        let acc = conf.acc.as_ref().unwrap();
        let best = idx.best().unwrap();
        assert_eq!(best.id, conf.genesis);
        let coins = idx.coins(&acc).unwrap();
        assert_eq!(1, coins.len());
        assert_eq!(coins[0].cpk, acc.hash().unwrap());
        assert_eq!(coins[0].value, consts::coin(50));
        assert_eq!(coins[0].base, 1);
        assert_eq!(coins[0].height, 0);
    });
}

/// 区块链数据存储索引
pub struct BlkIndexer {
    cache: BlkCache,  //缓存
    leveldb: LevelDB, //索引数据库指针
    blk: Store,       //区块存储
    rev: Store,       //回退日志存储
    conf: Config,     //配置信息
}

/// 区块链接脚本执行签名验证
struct LinkExectorEnv<'a> {
    tx: &'a Tx,         //当前交易
    inv: &'a TxIn,      //当前输入
    outv: &'a TxOut,    //输入对应的输出
    coin: &'a CoinAttr, //输入引用的金额
}

impl<'a> ExectorEnv for LinkExectorEnv<'a> {
    fn verify_sign(&self, ele: &Ele) -> Result<bool, Error> {
        let a: Account = ele.try_into()?;
        a.verify("aaa".as_bytes())
    }
}

/// 数据文件分布说明
/// data  --- 数据根目录
///       --- block 区块内容目录 store存储
///       --- index 索引目录,金额记录,区块头 leveldb
impl BlkIndexer {
    /// 每个文件最大大小
    const MAX_FILE_SIZE: u32 = 1024 * 1024 * 512;
    /// 最高区块入口存储key
    const BEST_KEY: &'static str = "__best__key__";
    fn new(conf: &Config) -> Result<Self, Error> {
        let dir = &conf.dir;
        util::miss_create_dir(&dir)?;
        let index = String::from(dir) + "/index";
        util::miss_create_dir(&index)?;
        let block = String::from(dir) + "/block";
        util::miss_create_dir(&block)?;
        Ok(BlkIndexer {
            cache: BlkCache::default(),
            leveldb: LevelDB::open(Path::new(&index))?,
            blk: Store::new(dir, "blk", Self::MAX_FILE_SIZE)?,
            rev: Store::new(dir, "rev", Self::MAX_FILE_SIZE)?,
            conf: conf.clone(),
        })
    }
    /// 获取当前配置
    pub fn config(&self) -> &Config {
        &self.conf
    }
    /// 获取最高区块信息
    /// 不存在应该是没有区块记录
    pub fn best(&self) -> Result<Best, Error> {
        let key: IKey = Self::BEST_KEY.into();
        self.leveldb.get(&key)
    }
    /// 下个区块的高度
    pub fn next_height(&self) -> u32 {
        self.best().map_or(0, |v| v.height + 1)
    }
    /// 获取下个区块难度
    pub fn next_bits(&mut self) -> Result<u32, Error> {
        let best = self.best();
        if best.is_err() {
            return Ok(self.conf.pow_limit.compact());
        }
        let best = best.unwrap();
        if best.height == 0 {
            return Ok(self.conf.pow_limit.compact());
        }
        let last = self.get(&best.id.as_ref().into())?;
        let next = best.next();
        if next % self.conf.pow_span != 0 {
            return Ok(last.header.bits);
        }
        let prev = next - self.conf.pow_span;
        let prev = self.get(&prev.into())?;
        let ct = last.header.get_timestamp();
        let pt = prev.header.get_timestamp();
        Ok(self
            .conf
            .pow_limit
            .compute_bits(self.conf.pow_time, ct, pt, last.header.bits))
    }
    /// 获取账户对应的金额列表
    pub fn coins(&self, acc: &Account) -> Result<Vec<CoinAttr>, Error> {
        let mut coins: Vec<CoinAttr> = vec![];
        let hash = acc.get_address()?;
        let akey: IKey = hash.as_ref().into();
        let iter = &mut self.leveldb.iter(&akey);
        while iter.next() {
            let key = &iter.key();
            if !key.starts_with(&akey) {
                break;
            }
            let value: Option<CoinAttr> = iter.value();
            if value.is_none() {
                return Error::msg("coin value none");
            }
            let mut value = value.unwrap();
            value.fill_key(key)?;
            coins.push(value);
        }
        Ok(coins)
    }
    /// 获取属性信息
    pub fn attr<T>(&self, k: &IKey) -> Result<T, Error>
    where
        T: Serializer + Default,
    {
        self.leveldb.get(k)
    }
    /// 从索引中获取区块
    /// 获取的区块只能读取
    pub fn get(&mut self, k: &IKey) -> Result<Arc<Block>, Error> {
        //u32 height读取,对应一个hashid
        if k.is_height_key() {
            let ref ik: Hasher = self.leveldb.get(k)?;
            return self.get(&ik.into());
        }
        //如果缓存中存在
        if let Ok(v) = self.cache.get(k) {
            return Ok(v);
        }
        //从数据库查询并加入缓存
        let attr: BlkAttr = self.attr(k)?;
        //读取区块数据
        let buf = self.blk.pull(&attr.blk)?;
        //解析成区块
        let mut reader = Reader::new(&buf);
        let mut blk: Block = reader.decode()?;
        blk.attr = attr;
        //加入缓存并返回
        self.cache.put(k, &blk)
    }
    /// 检测区块的金额
    /// 这个检测的区块是新连入的区块,需要检测引用的coin是否在链中
    /// 并且检测签名是否正确
    fn check_block_amount(&mut self, next: u32, blk: &Block) -> Result<(), Error> {
        //coinbase输出，输入, 输出, 交易费, 区块奖励
        //1.每个交易的输出<=输入,差值就是交易费用，可包含在coinbase输出中
        //3.coinbase输出 <= (区块奖励+交易费)
        let (mut cfee, mut ofee, mut ifee, mut tfee, rfee) =
            (0, 0, 0, 0, self.conf.compute_reward(next));
        for tx in blk.txs.iter() {
            for inv in tx.ins.iter() {
                //coinbase交易不包含金额信息
                if inv.is_coinbase() {
                    continue;
                }
                //获取消费地址hasher
                let addr = inv.get_address()?;
                //获取引用的输出
                let outv = self.get_txin_ref_txout(&inv)?;
                //地址是否一致
                if addr != outv.get_address()? {
                    return Error::msg("cost addr != out addr");
                }
                //获取引用的金额
                let coin = self.get_coin(&addr, &inv.out, inv.idx)?;
                //地址是否一致
                if addr != coin.get_address()? {
                    return Error::msg("cost addr != coin addr");
                }
                //金额是否一致
                if outv.value != coin.value {
                    return Error::msg("out value != coin value");
                }
                //金额是否成熟
                if !coin.is_matured(next) {
                    return Error::msg("coin not matured");
                }
                //消耗
                ifee += coin.value;
                //验证签名
                let env = &LinkExectorEnv {
                    tx: &tx,
                    inv: &inv,
                    outv: &outv,
                    coin: &coin,
                };
                let mut script = inv.script.clone();
                let script = script.concat(&outv.script);
                let mut exector = Exector::new();
                exector.exec(&script, env)?;
            }
            for outv in tx.outs.iter() {
                if tx.is_coinbase() {
                    cfee += outv.value;
                } else {
                    ofee += outv.value;
                }
            }
            if !consts::is_valid_amount(ifee) || !consts::is_valid_amount(ofee) {
                return Error::msg("ifee or ofee error");
            }
            if ifee > ofee {
                return Error::msg("tx input fee > tx out fee");
            }
            //累加交易费
            tfee += ifee - ofee;
            if !consts::is_valid_amount(tfee) {
                return Error::msg("tfee  error");
            }
            ifee = 0;
            ofee = 0;
        }
        if !consts::is_valid_amount(cfee) || !consts::is_valid_amount(rfee) {
            return Error::msg("cfee or rfee error");
        }
        //coinbase输出金额不能 > 奖励 + 交易费
        if cfee > rfee + tfee {
            return Error::msg("cfee > rfee + tfee");
        }
        Ok(())
    }
    /// 链接一个新的区块
    /// 返回顶部区块信息
    /// 写入的数据:
    /// best 顶部区块id和高度
    /// height->block id 高度对应的区块id
    /// block id->block attr 区块id对应的区块信息
    /// blk data 区块数据
    /// rev data 回退数据
    pub fn link(&mut self, blk: &Block) -> Result<Best, Error> {
        //检测基本数据
        blk.check_value(self)?;
        let id = blk.id()?;
        let ref key: IKey = id.as_ref().into();
        if self.leveldb.has(key) {
            return Error::msg("block exists");
        }
        //检测工作难度是否达到设置的要求
        if !id.verify_pow(&self.conf.pow_limit, blk.header.bits) {
            return Error::msg("block bits error");
        }
        //计算并检测下个区块难度
        let bits = self.next_bits()?;
        if blk.header.bits != bits {
            return Error::msg("link block bits error");
        }
        //开始写入
        let mut batch = IBatch::new(true);
        //最新best数据
        let mut best = Best {
            id: id.clone(),
            height: u32::MAX,
        };
        //获取顶部区块
        match self.best() {
            Ok(top) => {
                //获取上个区块信息
                let prev = self.get(&top.id_key())?;
                //看当前prev是否指向上个区块
                if blk.header.prev != prev.id()? {
                    return Error::msg("block prev != prev.id");
                }
                best.height = top.height + 1;
                //写入新的并保存旧的到回退数据
                batch.set(&Self::BEST_KEY.into(), &best, &top);
            }
            _ => {
                //第一个区块符合配置的上帝区块就直接写入
                if id != self.conf.genesis {
                    return Error::msg("first block not config genesis");
                }
                best.height = 0;
                batch.put(&Self::BEST_KEY.into(), &best);
            }
        }
        //检测区块的金额和签名
        self.check_block_amount(best.height, blk)?;
        //高度对应的区块id
        batch.put(&best.height_key(), &best.id);
        //每个交易对应的区块信息和位置
        for (i, tx) in blk.txs.iter().enumerate() {
            let txid = &tx.id()?;
            //如果此交易已经存在
            if self.leveldb.has(&txid.into()) {
                return Error::msg("txid exists block index");
            }
            let iv = TxAttr {
                blk: id.clone(), //此交易指向的区块
                idx: i as u16,   //在区块的位置
            };
            batch.put(&txid.into(), &iv);
            self.write_tx_index(&mut batch, &best, &tx)?;
        }
        //id对应的区块头属性
        let mut attr = BlkAttr::default();
        //当前区块头
        attr.bhv = blk.header.clone();
        //当前区块高度
        attr.hhv = best.height;
        //获取区块数据,回退数据并写入
        let blkwb = blk.bytes();
        let revwb = batch.reverse();
        //写二进制数据(区块内容和回退数据)
        attr.blk = self.blk.push(blkwb.bytes())?;
        attr.rev = self.rev.push(revwb.bytes())?;
        //写入区块id对应的区块头属性,这个不会包含在回退数据中,回退时删除数据
        batch.put(key, &attr);
        //批量写入
        self.leveldb.write(&batch, true)?;
        Ok(best)
    }
    /// 获取交易信息
    fn get_tx(&mut self, id: &Hasher) -> Result<Tx, Error> {
        //获取交易对应的存储属性
        let attr: TxAttr = self.attr(&id.as_ref().into())?;
        //获取对应的区块信息
        let blk = self.get(&attr.blk.as_ref().into())?;
        //获取对应的交易信息
        let tx = blk.get_tx(attr.idx as usize)?;
        Ok(tx.clone())
    }
    /// 获取输入引用的输出
    fn get_txin_ref_txout(&mut self, inv: &TxIn) -> Result<TxOut, Error> {
        //获取交易对应的存储属性
        let attr: TxAttr = self.attr(&inv.out.as_ref().into())?;
        //获取对应的区块信息
        let blk = self.get(&attr.blk.as_ref().into())?;
        //获取对应的交易信息
        let tx = blk.get_tx(attr.idx as usize)?;
        //获取对应的输出
        let outv = tx.get_out(inv.idx as usize)?;
        Ok(outv.clone())
    }
    /// 获取输入引用的金额
    pub fn get_txin_ref_coin(&mut self, inv: &TxIn) -> Result<CoinAttr, Error> {
        //获取交易对应的存储属性
        let attr: TxAttr = self.attr(&inv.out.as_ref().into())?;
        //获取区块信息
        let blk = self.get(&attr.blk.as_ref().into())?;
        //获取对应的交易信息
        let tx = blk.get_tx(attr.idx as usize)?;
        //获取对应的输出
        let outv = tx.get_out(inv.idx as usize)?;
        //创建coin信息返回
        let mut coin = CoinAttr::default();
        coin.cpk = outv.get_address()?;
        coin.tx = tx.id()?;
        coin.idx = inv.idx;
        coin.value = outv.value;
        if tx.is_coinbase() {
            coin.base = 1;
        }
        coin.height = blk.attr.hhv;
        Ok(coin)
    }
    /// 获取金额信息
    /// acc: 账户hash id
    /// tx:交易hash id
    /// idx:输出未知
    pub fn get_coin(&mut self, acc: &Hasher, tx: &Hasher, idx: u16) -> Result<CoinAttr, Error> {
        let mut coin = CoinAttr::default();
        coin.cpk = acc.clone();
        coin.tx = tx.clone();
        coin.idx = idx;
        let mut cv: CoinAttr = self.attr(&coin.key())?;
        cv.cpk = coin.cpk;
        cv.tx = coin.tx;
        cv.idx = coin.idx;
        Ok(cv)
    }
    /// 写入交易索引
    fn write_tx_index(
        &mut self,
        batch: &mut IBatch, //批事务写入
        best: &Best,        //当前生成区块的高度
        tx: &Tx,            //当前区块交易
    ) -> Result<(), Error> {
        let mut base: u8 = 0;
        if tx.is_coinbase() {
            base = 1;
        }
        //输入对应消耗金额
        for inv in tx.ins.iter() {
            //coinbase输入不存在金额消耗
            if inv.is_coinbase() {
                continue;
            }
            //获取引用的金额
            let coin = self.get_txin_ref_coin(&inv)?;
            //在当前高度上是否成熟
            if !coin.is_matured(best.height) {
                return Error::msg("ref coin not matured");
            }
            //删除coin并且存入回退数据
            batch.del(&coin.key(), Some(&coin));
        }
        //输出对应获取的金额
        for (i, outv) in tx.outs.iter().enumerate() {
            let mut coin = CoinAttr::default();
            coin.cpk = outv.get_address()?;
            coin.tx = tx.id()?;
            coin.idx = i as u16;
            coin.value = outv.value;
            coin.base = base;
            coin.height = best.height;
            batch.put_attr(&coin);
        }
        Ok(())
    }
    /// 回退一个区块,回退多个连续调用此方法
    /// 返回被回退的区块
    pub fn pop(&mut self) -> Result<Block, Error> {
        //获取区块链最高区块属性
        let best = self.best()?;
        let ref idkey = best.id_key();
        let attr: BlkAttr = self.leveldb.get(idkey)?;
        //读取区块数据
        let buf = self.blk.pull(&attr.blk)?;
        let blk: Block = Reader::unpack(&buf)?;
        //读取回退数据
        let buf = self.rev.pull(&attr.rev)?;
        let mut batch: IBatch = buf[..].try_into()?;
        //删除最后一个区块
        batch.del::<Block>(idkey, None);
        //删除缓存
        self.cache.pop(idkey);
        //批量写入
        self.leveldb.write(&batch, true)?;
        Ok(blk)
    }
}

/// 链线程安全封装
pub struct Chain(RwLock<BlkIndexer>);

impl Chain {
    /// lock read process
    fn do_read<R, F>(&self, f: F) -> Result<R, Error>
    where
        F: FnOnce(&BlkIndexer) -> Result<R, Error>,
    {
        self.0.read().map_or_else(Error::std, |ref v| f(v))
    }
    /// lock write process
    fn do_write<R, F>(&self, f: F) -> Result<R, Error>
    where
        F: FnOnce(&mut BlkIndexer) -> Result<R, Error>,
    {
        self.0.write().map_or_else(Error::std, |ref mut v| f(v))
    }
    /// 获取当前配置
    pub fn config(&self) -> Result<Config, Error> {
        self.do_read(|v| Ok(v.config().clone()))
    }
    /// 创建指定路径存储的链
    pub fn new(conf: &Config) -> Result<Self, Error> {
        Ok(Chain(RwLock::new(BlkIndexer::new(conf)?)))
    }
    /// 获取区块链顶部信息
    pub fn best(&self) -> Result<Best, Error> {
        self.do_read(|v| v.best())
    }
    /// 获取属性信息,不缓存
    pub fn attr<T>(&self, k: &IKey) -> Result<T, Error>
    where
        T: Serializer + Default,
    {
        self.do_read(|v| v.attr(k))
    }
    /// 获取账户对应的金额列表
    pub fn coins(&self, acc: &Account) -> Result<Vec<CoinAttr>, Error> {
        self.do_read(|v| v.coins(acc))
    }
    /// 获取交易信息
    pub fn get_tx(&self, k: &IKey) -> Result<Tx, Error> {
        let attr: TxAttr = self.attr(k)?;
        let blk = self.get(&attr.blk.as_ref().into())?;
        if attr.idx >= blk.txs.len() as u16 {
            return Error::msg("idx outbound block txs len");
        }
        Ok(blk.txs[attr.idx as usize].clone())
    }
    /// 获取区块信息
    /// k可以是高度或者id
    pub fn get(&self, k: &IKey) -> Result<Arc<Block>, Error> {
        self.do_write(|v| v.get(k))
    }
    /// 链接一个新区块到链上
    pub fn link(&self, blk: &Block) -> Result<Best, Error> {
        self.do_write(|ctx| ctx.link(blk))
    }
    /// 弹出一个区块
    pub fn pop(&self) -> Result<Block, Error> {
        self.do_write(|v| v.pop())
    }
}

#[test]
fn test_indexer_thread() {
    use std::sync::Arc;
    use std::{thread, time};
    Config::test(|conf, idx| {
        let indexer = Arc::new(idx);
        for b in 0..10 {
            let idx = indexer.clone();
            let conf = conf.clone();
            thread::spawn(move || {
                let iv = b;
                let b1 = conf
                    .create_block(Hasher::zero(), iv, "", |blk| {
                        let best = idx.best().unwrap();
                        blk.header.prev = best.id;
                    })
                    .unwrap();
                idx.link(&b1).unwrap();
                let id = b1.id().unwrap();
                let b2 = idx.get(&id.as_ref().into()).unwrap();
                assert_eq!(b1, *b2);
            });
        }
        thread::sleep(time::Duration::from_secs(1));
    })
}

#[test]
fn test_simple_link_pop() {
    Config::test(|conf, idx| {
        let best = idx.best().unwrap();
        assert_eq!(0, best.height);
        for i in 0u32..=10 {
            let b1 = conf
                .create_block(Hasher::zero(), i + 1, "", |blk| {
                    let best = idx.best().unwrap();
                    blk.header.prev = best.id;
                })
                .unwrap();
            idx.link(&b1).unwrap();
        }
        let best = idx.best().unwrap();
        assert_eq!(11, best.height);
        for i in 0u32..=10 {
            let best = idx.best().unwrap();
            assert_eq!(11 - i, best.height);
            idx.pop().unwrap();
        }
        let best = idx.best().unwrap();
        assert_eq!(0, best.height);
        assert_eq!(best.id, conf.genesis);
    });
}

/// 线程安全的区块LRU缓存实现
pub struct BlkCache {
    lru: LruCache<IKey, Arc<Block>>,
}

impl Default for BlkCache {
    fn default() -> Self {
        BlkCache::new(1024 * 10)
    }
}

impl BlkCache {
    /// 创建指定大小的lur缓存
    pub fn new(cap: usize) -> Self {
        BlkCache {
            lru: LruCache::new(cap),
        }
    }
    /// 获取缓存长度
    pub fn len(&self) -> usize {
        self.lru.len()
    }
    /// 加入缓存值
    /// 如果存在将返回旧值
    pub fn put(&mut self, k: &IKey, v: &Block) -> Result<Arc<Block>, Error> {
        let ret = Arc::new(v.clone());
        self.lru.put(k.clone(), ret.clone());
        Ok(ret.clone())
    }
    /// 从缓存获取值,不改变缓存状态
    pub fn peek<F>(&self, k: &IKey) -> Option<Arc<Block>> {
        self.lru.peek(k).map(|v| v.clone())
    }

    /// 检测指定的key是否存在
    pub fn contains(&self, k: &IKey) -> bool {
        self.lru.contains(k)
    }

    /// 从缓存移除数据
    pub fn pop(&mut self, k: &IKey) -> Option<Arc<Block>> {
        self.lru.pop(k)
    }
    /// 从缓存获取值并复制返回
    /// 复制对应的值返回
    pub fn get(&mut self, k: &IKey) -> Result<Arc<Block>, Error> {
        self.lru
            .get(k)
            .map(|v| v.clone())
            .ok_or(Error::error("not found"))
    }
}

#[test]
fn test_lru_write() {
    let c = &mut BlkCache::default();
    let blk = Block::default();
    c.put(&1u32.into(), &blk).unwrap();
    let v = c.get(&1u32.into()).unwrap();
    let ver = v.header.get_ver();
    assert_eq!(ver, 1);
}

#[test]
fn test_lru_cache() {
    let c = &mut BlkCache::default();
    let blk = Block::default();
    c.put(&1u32.into(), &blk).unwrap();
    let v = c.get(&1u32.into()).unwrap();
    assert_eq!(v.as_ref(), &Block::default());
    assert_eq!(1, c.len());
    let v = c.pop(&1u32.into()).unwrap();
    assert_eq!(v.as_ref(), &Block::default());
    assert_eq!(0, c.len());
}
