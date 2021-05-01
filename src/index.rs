use crate::block::{Best, BlkAttr, Block, Checker, Tx, TxAttr};
use crate::bytes::IntoBytes;
use crate::config::Config;
use crate::consts;
use crate::errors::Error;
use crate::hasher::Hasher;
use crate::iobuf::{Reader, Serializer};
use crate::leveldb::{IBatch, LevelDB};
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

/// 区块链数据存储索引
pub struct BlkIndexer {
    cache: BlkCache,  //缓存
    leveldb: LevelDB, //索引数据库指针
    blk: Store,       //区块存储
    rev: Store,       //回退日志存储
    conf: Config,     //配置信息
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
        let attr: BlkAttr = self.leveldb.get(k)?;
        //读取区块数据
        let buf = self.blk.pull(&attr.blk)?;
        //解析成区块
        let mut reader = Reader::new(&buf);
        let blk: Block = reader.decode()?;
        //加入缓存并返回
        self.cache.put(k, &blk)
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
        let id = blk.id()?;
        let ref key: IKey = id.as_ref().into();
        if self.leveldb.has(key) {
            return Error::msg("block exists");
        }
        //开始写入
        let mut batch = IBatch::new(true);
        let mut best = Best {
            id: id.clone(),
            height: u32::MAX,
        };
        //获取顶部区块
        match self.best() {
            Ok(top) => {
                //其他区块检测上一个区块,现在也暂时直接写入//best配置写入
                best.height = top.height + 1;
                //写入新的并保存旧的到回退数据
                batch.set(&Self::BEST_KEY.into(), &best, &top);
            }
            _ => {
                //第一个区块符合配置的上帝区块就直接写入
                best.height = 0;
                batch.put(&Self::BEST_KEY.into(), &best);
            }
        }
        //高度对应的区块id
        batch.put(&best.height.into(), &best.id);
        //交易对应的区块信息和位置
        for (i, tx) in blk.txs.iter().enumerate() {
            let txid = &tx.id()?;
            let iv = TxAttr {
                blk: txid.clone(),
                idx: i as u16,
            };
            batch.put(&txid.into(), &iv);
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
        batch.put(&id.as_ref().into(), &attr);
        //批量写入
        self.leveldb.write(&batch, true)?;
        Ok(best)
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
        self.do_write(|v| {
            blk.check_value(v)?;
            v.link(blk)
        })
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
    let config = Arc::new(Config::test());
    let indexer = Arc::new(Chain::new(&config).unwrap());
    for b in 0..10 {
        let idx = indexer.clone();
        let conf = config.clone();
        thread::spawn(move || {
            let iv = b;
            let b1 = conf.create_block(iv, "", consts::coin(50)).unwrap();
            idx.link(&b1).unwrap();
            let id = b1.id().unwrap();
            let b2 = idx.get(&id.as_ref().into()).unwrap();
            assert_eq!(b1, *b2);
        });
    }
    thread::sleep(time::Duration::from_secs(1));
}

#[test]
fn test_simple_link_pop() {
    let config = Config::test();
    let idx = Chain::new(&config).unwrap();
    for i in 0u32..=10 {
        let b1 = config.create_block(i, "", consts::coin(50)).unwrap();
        idx.link(&b1).unwrap();
    }
    let best = idx.best().unwrap();
    assert_eq!(10, best.height);
    for i in 0u32..=10 {
        let best = idx.best().unwrap();
        assert_eq!(10 - i, best.height);
        idx.pop().unwrap();
    }
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
    pub fn has(&self, k: &IKey) -> bool {
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
