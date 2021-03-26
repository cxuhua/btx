use crate::block::{Best, Block};
use crate::bytes::IntoBytes;
use crate::errors::Error;
use crate::hasher::Hasher;
use crate::leveldb::LevelDB;
use crate::store::Store;
use crate::util;
use bytes::BufMut;
use core::hash;
use db_key::Key;
use lru::LruCache;
use std::cmp::{Eq, PartialEq};
use std::convert::Into;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;

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

/// 区块链存储索引
/// 优先从缓存获取,失败从数据库获取
pub trait Indexer: Sized {
    /// 根据k获取区块
    fn get(&self, _k: &IKey) -> Option<Arc<Block>>;
    /// 获取最高区块信息
    /// 不存在应该是没有区块记录
    fn best(&self) -> Option<Best>;
}

pub struct BlkIndexer {
    root: String,      //数据根目录
    block: String,     //内容存储
    index: String,     //索引目录
    cache: BlkCache,   //缓存
    idx: LevelDB,      //索引数据库指针
    blk: Mutex<Store>, //区块存储索引
    rev: Mutex<Store>, //回退日志存储索引
}

impl Indexer for BlkIndexer {
    /// 获取最高区块信息
    /// 不存在应该是没有区块记录
    fn best(&self) -> Option<Best> {
        let key: IKey = Self::BEST_KEY.into();
        self.idx.get(&key)
    }
    /// 从索引中获取区块
    fn get(&self, k: &IKey) -> Option<Arc<Block>> {
        //如果缓存中存在
        if let Some(v) = self.cache.get(k) {
            return Some(v);
        }
        //从数据库查询并加入缓存
        let v: Option<Block> = self.idx.get(k);
        if v.is_none() {
            return None;
        }
        let blk = v.unwrap();
        //加入缓存
        self.cache.put(k, &blk);
        //
        Some(Arc::new(blk))
    }
}

/// 数据文件分布说明
/// data  --- 数据根目录
///       --- block 区块内容目录 store存储
///       --- index 索引目录,金额记录,区块头 leveldb
impl BlkIndexer {
    /// 每个文件最大大小
    const MAX_FILE_SIZE: u32 = 1024 * 1024 * 512;
    const BEST_KEY: &'static str = "__best__key__";
    /// 创建存储索引
    pub fn new(dir: &str) -> Result<Self, Error> {
        let idxpath = String::from(dir) + "/index";
        util::miss_create_dir(&idxpath)?;
        let blkpath = String::from(dir) + "/block";
        util::miss_create_dir(&blkpath)?;
        Ok(BlkIndexer {
            root: String::from(dir),
            block: blkpath.clone(),
            index: idxpath.clone(),
            cache: BlkCache::default(),
            idx: LevelDB::open(Path::new(&idxpath))?,
            blk: Mutex::new(Store::new(dir, "blk", Self::MAX_FILE_SIZE)?),
            rev: Mutex::new(Store::new(dir, "rev", Self::MAX_FILE_SIZE)?),
        })
    }
}

#[test]
fn test_block_indexer() {
    BlkIndexer::new("./tmp").unwrap();
}

/// lru线程安全的区块缓存实现
pub struct BlkCache {
    lru: Mutex<LruCache<IKey, Arc<Block>>>,
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
            lru: Mutex::new(LruCache::new(cap)),
        }
    }
    /// 获取缓存长度
    pub fn len(&self) -> usize {
        let wl = self.lru.lock().unwrap();
        wl.len()
    }
    /// 加入缓存值
    /// 如果存在将返回旧值
    pub fn put(&self, k: &IKey, v: &Block) -> Option<Arc<Block>> {
        let mut wl = self.lru.lock().unwrap();
        wl.put(k.clone(), Arc::new(v.clone()))
    }
    /// 从缓存获取值,不改变缓存状态
    pub fn peek<F>(&self, k: &IKey) -> Option<Arc<Block>> {
        let wl = self.lru.lock().unwrap();
        wl.peek(k).map(|v| v.clone())
    }

    /// 检测指定的key是否存在
    pub fn has(&self, k: &IKey) -> bool {
        let wl = self.lru.lock().unwrap();
        wl.contains(k)
    }

    /// 从缓存移除数据
    pub fn pop(&self, k: &IKey) -> Option<Arc<Block>> {
        let mut wl = self.lru.lock().unwrap();
        wl.pop(k)
    }

    /// 从缓存获取值并复制返回
    /// 复制对应的值返回
    pub fn get(&self, k: &IKey) -> Option<Arc<Block>> {
        let mut wl = self.lru.lock().unwrap();
        wl.get(k).map(|v| v.clone())
    }
}

#[test]
fn test_lru_write() {
    let c = BlkCache::default();
    let blk = Block::default();
    c.put(&1u32.into(), &blk);
    let v = c.get(&1u32.into()).unwrap();
    let ver = v.header.get_ver();
    assert_eq!(ver, 1);
}

#[test]
fn test_lru_cache() {
    let c = BlkCache::default();
    let blk = Block::default();
    c.put(&1u32.into(), &blk);
    let v = c.get(&1u32.into()).unwrap();
    assert_eq!(v.as_ref(), &Block::default());
    assert_eq!(1, c.len());
    let v = c.pop(&1u32.into()).unwrap();
    assert_eq!(v.as_ref(), &Block::default());
    assert_eq!(0, c.len());
}

#[test]
fn test_lru_thread() {
    use std::sync::Arc;
    use std::{thread, time};
    let c = Arc::new(BlkCache::default());
    for _ in 0..10 {
        let c1 = c.clone();
        thread::spawn(move || {
            c1.put(&1u32.into(), &Block::default());
            let v = c1.get(&1u32.into()).unwrap();
            assert_eq!(v.as_ref(), &Block::default());
            assert_eq!(1, c1.len());
        });
    }
    thread::sleep(time::Duration::from_secs(1));
}
