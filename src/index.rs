use crate::block::Block;
use crate::bytes::IntoBytes;
use crate::errors::Error;
use crate::hasher::Hasher;
use bytes::BufMut;
use core::hash;
use db_key::Key;
use lru::LruCache;
use std::cmp::{Eq, PartialEq};
use std::convert::Into;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
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

/// 按高度查询区块
impl From<u32> for IKey {
    fn from(v: u32) -> Self {
        let mut key = IKey(vec![]);
        key.0.put_u32_le(v);
        key
    }
}

/// 按高度查询区块
impl From<&[u8]> for IKey {
    fn from(v: &[u8]) -> Self {
        IKey(v.to_vec())
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
    //失败包含前缀
    pub fn starts_with(&self, prefix: &IKey) -> bool {
        self.0.starts_with(&prefix.0)
    }
}

/// 区块链存储索引
/// 优先从缓存获取,失败从数据库获取
pub trait Indexer: Sized {
    /// 根据k获取区块
    fn get<K>(&self, _k: K) -> Option<Arc<Block>>
    where
        K: Into<IKey>;
    /// 获取区块缓存器
    fn cache(&self) -> Option<&BlkCache>;
}

pub struct BlkIndexer {
    root: String,    //数据根目录
    entry: String,   //入口数据库
    block: String,   //内容存储
    index: String,   //索引目录
    cache: BlkCache, //缓存
}

impl Indexer for BlkIndexer {
    fn get<K>(&self, k: K) -> Option<Arc<Block>>
    where
        K: Into<IKey>,
    {
        //如果缓存中存在
        if let Some(v) = self.cache.get(k) {
            return Some(v);
        }
        //从数据库查询并加入缓存
        None
    }
    fn cache(&self) -> Option<&BlkCache> {
        None
    }
}

/// 数据文件分布说明
/// data  --- 数据根目录
///       --- entry 入口索引文件 leveldb
///       --- block 区块内容目录 store存储
///       --- index 索引目录,金额记录,区块头 leveldb
impl BlkIndexer {
    /// 如果目录不存在直接创建
    fn miss_create_dir(&self, dir: &str) -> Result<(), Error> {
        let p = Path::new(dir);
        //目录是否存在
        let has = match fs::metadata(p).map(|v| v.is_dir()) {
            Ok(v) => v,
            _ => false,
        };
        if has {
            return Ok(());
        }
        match fs::create_dir(&p) {
            Ok(_) => Ok(()),
            Err(err) => Error::std(err),
        }
    }
    /// 初始化
    fn init(&self) -> Result<(), Error> {
        //root dir
        self.miss_create_dir(&self.root)?;
        //entry dir
        self.miss_create_dir(&self.entry)?;
        //block dir
        self.miss_create_dir(&self.block)?;
        //index dir
        self.miss_create_dir(&self.index)?;
        //entry leveldb
        //block store
        //index leveldb
        Ok(())
    }

    /// 创建存储索引
    pub fn new(dir: &str) -> Result<Self, Error> {
        let indexer = BlkIndexer {
            root: String::from(dir),
            entry: String::from(dir) + "/entry",
            block: String::from(dir) + "/block",
            index: String::from(dir) + "/index",
            cache: BlkCache::default(),
        };
        indexer.init()?;
        Ok(indexer)
    }
}

#[test]
fn test_block_indexer() {
    BlkIndexer::new("/Users/xuhua/btx/data").unwrap();
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
        match self.lru.lock() {
            Ok(w) => w.len(),
            _ => panic!("lock failed"),
        }
    }
    /// 加入缓存值
    /// 如果存在将返回旧值
    pub fn put<K>(&self, k: K, v: Block) -> Option<Arc<Block>>
    where
        K: Into<IKey>,
    {
        match self.lru.lock() {
            Ok(mut w) => w.put(k.into(), Arc::new(v)),
            _ => panic!("lock failed"),
        }
    }
    /// 从缓存获取值,不改变缓存状态
    pub fn peek<K, F>(&self, k: K) -> Option<Arc<Block>>
    where
        K: Into<IKey>,
    {
        match self.lru.lock() {
            Ok(w) => match w.peek(&k.into()) {
                Some(v) => Some(v.clone()),
                _ => None,
            },
            _ => panic!("lock failed"),
        }
    }

    /// 检测指定的key是否存在
    pub fn has<K>(&self, k: K) -> bool
    where
        K: Into<IKey>,
    {
        match self.lru.lock() {
            Ok(w) => w.contains(&k.into()),
            _ => panic!("lock failed"),
        }
    }

    /// 从缓存移除数据
    pub fn pop<K>(&self, k: K) -> Option<Arc<Block>>
    where
        K: Into<IKey>,
    {
        match self.lru.lock() {
            Ok(mut w) => w.pop(&k.into()),
            _ => panic!("lock failed"),
        }
    }

    /// 从缓存获取值并复制返回
    /// 复制对应的值返回
    pub fn get<K>(&self, k: K) -> Option<Arc<Block>>
    where
        K: Into<IKey>,
    {
        match self.lru.lock() {
            Ok(mut w) => match w.get(&k.into()) {
                Some(v) => Some(v.clone()),
                _ => None,
            },
            _ => panic!("lock failed"),
        }
    }
}

#[test]
fn test_lru_write() {
    let c = BlkCache::default();
    let blk = Block::default();
    c.put(1u32, blk);
    let v = c.get(1u32).unwrap();
    let ver = v.header.get_ver();
    assert_eq!(ver, 1);
}

#[test]
fn test_lru_cache() {
    let c = BlkCache::default();
    let blk = Block::default();
    c.put(1u32, blk);
    let v = c.get(1u32).unwrap();
    assert_eq!(v.as_ref(), &Block::default());
    assert_eq!(1, c.len());
    let v = c.pop(1u32).unwrap();
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
            c1.put(1u32, Block::default());
            let v = c1.get(1u32).unwrap();
            assert_eq!(v.as_ref(), &Block::default());
            assert_eq!(1, c1.len());
        });
    }
    thread::sleep(time::Duration::from_secs(1));
}
