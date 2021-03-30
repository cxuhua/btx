use crate::block::{Best, BlkAttr, Block, Checker};
use crate::bytes::IntoBytes;
use crate::errors::Error;
use crate::hasher::Hasher;
use crate::iobuf::Reader;
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
pub trait IndexerEvent {
    /// 从链移除前
    fn on_pop(&mut self, _blk: &Block, _batch: &mut IBatch) -> Result<(), Error> {
        Ok(())
    }
    /// 从链移除后
    fn on_poped(&mut self, _blk: &Block) {}
    /// 将要链接前
    fn on_link(&mut self, _blk: &Block, _batch: &mut IBatch) -> Result<(), Error> {
        Ok(())
    }
    /// 将要链接后
    fn on_linked(&mut self, _blk: &Block) {}
}

/// 区块链数据存储索引
pub struct BlkIndexer<'a> {
    cache: BlkCache,                         //缓存
    idx: LevelDB,                            //索引数据库指针
    blk: Mutex<Store>,                       //区块存储
    rev: Mutex<Store>,                       //回退日志存储
    event: Option<&'a mut dyn IndexerEvent>, // 事件回调
}

/// 数据文件分布说明
/// data  --- 数据根目录
///       --- block 区块内容目录 store存储
///       --- index 索引目录,金额记录,区块头 leveldb
impl<'a> BlkIndexer<'a> {
    /// 设置监听事件
    pub fn set_event(&mut self, ob: &'a mut dyn IndexerEvent) {
        self.event = Some(ob);
    }
    /// 每个文件最大大小
    const MAX_FILE_SIZE: u32 = 1024 * 1024 * 512;
    /// 最高区块入口存储key
    const BEST_KEY: &'static str = "__best__key__";
    /// 创建存储索引
    pub fn new(dir: &str) -> Result<Self, Error> {
        let idxpath = String::from(dir) + "/index";
        util::miss_create_dir(&idxpath)?;
        let blkpath = String::from(dir) + "/block";
        util::miss_create_dir(&blkpath)?;
        Ok(BlkIndexer {
            cache: BlkCache::default(),
            idx: LevelDB::open(Path::new(&idxpath))?,
            blk: Mutex::new(Store::new(dir, "blk", Self::MAX_FILE_SIZE)?),
            rev: Mutex::new(Store::new(dir, "rev", Self::MAX_FILE_SIZE)?),
            event: None,
        })
    }
    /// 获取最高区块信息
    /// 不存在应该是没有区块记录
    pub fn best(&self) -> Result<Best, Error> {
        let key: IKey = Self::BEST_KEY.into();
        self.idx.get(&key)
    }
    /// 从索引中获取区块
    pub fn get(&self, k: &IKey) -> Result<Arc<Block>, Error> {
        //u32 height读取,对应一个hashid
        if k.len() == 4 {
            if let Ok(ref ik) = self.idx.get::<Hasher>(k) {
                return self.get(&ik.into());
            }
        }
        //如果缓存中存在
        if let Ok(v) = self.cache.get(k) {
            return Ok(v);
        }
        //从数据库查询并加入缓存
        let attr: BlkAttr = self.idx.get(k)?;
        //读取区块数据
        let buf = self
            .blk
            .lock()
            .map_or_else(Error::std, |ref mut v| v.pull(&attr.blk))?;
        //解析成区块
        let mut reader = Reader::new(&buf);
        let blk: Block = reader.decode()?;
        //加入缓存并返回
        self.cache.put(k, &blk)
    }
    /// 链接一个新的区块
    /// 返回顶部区块信息
    /// 写入的数据: best,height->block id,block id->block attr,blk data,rev data
    pub fn link(&mut self, blk: &Block) -> Result<Best, Error> {
        blk.check_value()?;
        let id = blk.id()?;
        let ref key: IKey = id.as_ref().into();
        if self.idx.has(key) {
            return Error::msg("block exists");
        }
        //开始写入
        let mut batch = IBatch::new(true);
        let mut best = Best::default();
        best.id = id.clone();
        match self.best() {
            Ok(prev) => {
                //其他区块检测上一个区块,现在也暂时直接写入//best配置写入
                best.height = prev.height + 1;
                //写入新的并保存旧的到回退数据
                batch.replace(&Self::BEST_KEY.into(), &best, &prev);
            }
            Err(_) => {
                //第一个区块符合配置的上帝区块就直接写入
                best.height = 0;
                batch.put(&Self::BEST_KEY.into(), &best);
            }
        }
        //高度对应的区块id
        batch.put(&best.height.into(), &best.id);
        //id对应的区块头属性
        let mut attr = BlkAttr::default();
        //当前区块头
        attr.bhv = blk.header.clone();
        //当前区块高度
        attr.hhv = best.height;
        //获取区块数据,回退数据并写入
        let blkwb = blk.bytes();
        let revwb = batch.reverse();
        //link事件,写入数据之前调用
        if let Some(ref mut ev) = self.event {
            ev.on_link(blk, &mut batch)?;
        }
        //写二进制数据
        attr.blk = self
            .blk
            .lock()
            .map_or_else(Error::std, |ref mut v| v.push(blkwb.bytes()))?;
        attr.rev = self
            .rev
            .lock()
            .map_or_else(Error::std, |ref mut v| v.push(revwb.bytes()))?;
        //写入区块id对应的区块头属性,这个不会包含在回退数据中,回退时删除数据
        batch.put(&id.as_ref().into(), &attr);
        //批量写入
        self.idx.write(&batch, true)?;
        //链接成功事件
        if let Some(ref mut ev) = self.event {
            ev.on_linked(blk);
        }
        Ok(best)
    }
    /// 回退一个区块,回退多个连续调用此方法
    pub fn pop(&mut self) -> Result<Block, Error> {
        //获取区块链最高区块属性
        let best = self.best()?;
        let attr: BlkAttr = self.idx.get(&best.id.as_ref().into())?;
        //读取区块数据
        let buf = self
            .blk
            .lock()
            .map_or_else(Error::std, |ref mut v| v.pull(&attr.blk))?;
        let mut reader = Reader::new(&buf);
        let blk: Block = reader.decode()?;
        //读取回退数据
        let buf = self
            .rev
            .lock()
            .map_or_else(Error::std, |ref mut v| v.pull(&attr.rev))?;
        let mut batch: IBatch = buf[..].try_into()?;
        //删除最后一个区块
        batch.del::<Block>(&best.id.as_ref().into(), None);
        //pop事件
        if let Some(ref mut ev) = self.event {
            ev.on_pop(&blk, &mut batch)?;
        }
        //删除缓存
        self.cache.pop(&best.id.as_ref().into());
        //批量写入
        self.idx.write(&batch, true)?;
        //移除成功事件
        if let Some(ref mut ev) = self.event {
            ev.on_poped(&blk);
        }
        Ok(blk)
    }
}

#[test]
fn test_simple_link_pop() {
    struct Event(u32);
    impl IndexerEvent for Event {
        fn on_pop(&mut self, _blk: &Block, _batch: &mut IBatch) -> Result<(), Error> {
            self.0 += 1;
            Ok(())
        }
    }
    use tempdir::TempDir;
    let tmp = TempDir::new("db").unwrap();
    let mut idx = BlkIndexer::new(tmp.path().to_str().unwrap()).unwrap();
    let mut ev = Event(0);
    idx.set_event(&mut ev);
    for i in 0u32..=10 {
        let mut b1 = Block::default();
        b1.header.time = i;
        idx.link(&b1).unwrap();
    }
    for i in 0u32..=10 {
        let mut b1 = Block::default();
        b1.header.time = i;
        let id = b1.id().unwrap();
        let b2 = idx.get(&id.as_ref().into()).unwrap();
        assert_eq!(b1, *b2);
        let b3 = idx.get(&i.into()).unwrap();
        assert_eq!(b1, *b3);
    }
    let best = idx.best().unwrap();
    assert_eq!(10, best.height);
    for i in 0u32..=10 {
        let best = idx.best().unwrap();
        assert_eq!(10 - i, best.height);
        idx.pop().unwrap();
    }
    assert_eq!(11, ev.0);
}

/// 线程安全的区块LRU缓存实现
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
    pub fn put(&self, k: &IKey, v: &Block) -> Result<Arc<Block>, Error> {
        let mut wl = self.lru.lock().unwrap();
        let ret = Arc::new(v.clone());
        wl.put(k.clone(), ret.clone());
        Ok(ret.clone())
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
    pub fn get(&self, k: &IKey) -> Result<Arc<Block>, Error> {
        let mut wl = self.lru.lock().unwrap();
        wl.get(k)
            .map(|v| v.clone())
            .ok_or(Error::error("not found"))
    }
}

#[test]
fn test_lru_write() {
    let c = BlkCache::default();
    let blk = Block::default();
    c.put(&1u32.into(), &blk).unwrap();
    let v = c.get(&1u32.into()).unwrap();
    let ver = v.header.get_ver();
    assert_eq!(ver, 1);
}

#[test]
fn test_lru_cache() {
    let c = BlkCache::default();
    let blk = Block::default();
    c.put(&1u32.into(), &blk).unwrap();
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
            c1.put(&1u32.into(), &Block::default()).unwrap();
            let v = c1.get(&1u32.into()).unwrap();
            assert_eq!(v.as_ref(), &Block::default());
            assert_eq!(1, c1.len());
        });
    }
    thread::sleep(time::Duration::from_secs(1));
}
