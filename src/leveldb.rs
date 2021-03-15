use crate::errors::Error;
use crate::index::IKey;
use crate::iobuf::{Reader, Serializer, Writer};
use leveldb::database::cache::Cache;
use leveldb::database::Database;
use leveldb::iterator::LevelDBIterator;
use leveldb::iterator::{Iterable, Iterator, RevIterator};
use leveldb::kv::KV;
use leveldb::options::{Options, ReadOptions, WriteOptions};
use std::marker::PhantomData;
use std::path::Path;
/// kv数据索引存储定义
pub trait DB: Sized {
    /// 添加数据
    fn put<V>(&self, _k: &IKey, _v: V, _sync: bool) -> Result<(), Error>
    where
        V: Serializer;

    /// 获取数据
    fn get<V>(&self, _k: &IKey) -> Option<V>
    where
        V: Serializer + Default;

    /// 删除数据
    fn del(&self, _k: &IKey) -> Result<(), Error>;

    /// KV正向迭代器
    fn iter<'a>(&'a self, prefix: &'a IKey) -> Iter<'a, Iterator<'a, IKey>>;
}

/// 线程安全的kv数据存储实现
pub struct LevelDB {
    db: Database<IKey>,
}

pub struct Iter<'a, T> {
    iter: Box<T>,
    marker: PhantomData<&'a T>,
}

impl<'a> Iter<'a, Iterator<'a, IKey>> {
    /// 移动到某个key
    pub fn seek(&self, k: &'a IKey) {
        self.iter.seek(k);
    }
    /// 获取当前key
    pub fn key(&self) -> IKey {
        self.iter.key()
    }
    /// 获取当前值字节
    pub fn bytes(&self) -> Vec<u8> {
        self.iter.value()
    }
    /// 解码当前数据字节
    pub fn value<T>(&self) -> Option<T>
    where
        T: Serializer + Default,
    {
        let b = self.iter.value();
        match Reader::new(&b).decode() {
            Ok(v) => Some(v),
            Err(_) => None,
        }
    }
    /// 下一个
    pub fn next(&mut self) -> bool {
        self.iter.advance()
    }
    /// key是否包含from前缀
    pub fn has_prefix(&self) -> bool {
        if !self.iter.valid() {
            return false;
        }
        match self.iter.from_key() {
            Some(v) => self.key().starts_with(v),
            None => false,
        }
    }
}

#[test]
fn test_db_iter() {}

impl DB for LevelDB {
    /// 从小到大迭代
    fn iter<'a>(&'a self, prefix: &'a IKey) -> Iter<'a, Iterator<'a, IKey>> {
        let opts = ReadOptions::new();
        Iter {
            iter: Box::new(self.db.iter(opts).from(prefix)),
            marker: PhantomData,
        }
    }
    fn put<V>(&self, k: &IKey, v: V, sync: bool) -> Result<(), Error>
    where
        V: Serializer,
    {
        let wb = &mut Writer::default();
        v.encode(wb);
        let mut opts = WriteOptions::new();
        opts.sync = sync;
        match self.db.put(opts, k, wb.bytes()) {
            Ok(_) => Ok(()),
            Err(err) => Error::std(err),
        }
    }
    fn get<V>(&self, k: &IKey) -> Option<V>
    where
        V: Serializer + Default,
    {
        let opts = ReadOptions::new();
        match self.db.get(opts, k) {
            Ok(v) => match v {
                Some(v) => match Reader::new(&v).decode() {
                    Ok(v) => Some(v),
                    Err(_) => None,
                },
                None => None,
            },
            Err(_) => None,
        }
    }
    fn del(&self, k: &IKey) -> Result<(), Error> {
        let mut opts = WriteOptions::new();
        opts.sync = false;
        match self.db.delete(opts, k) {
            Ok(_) => Ok(()),
            Err(err) => Error::std(err),
        }
    }
}

struct ReverseComparator<K> {
    marker: PhantomData<K>,
}

impl LevelDB {
    /// 指定目录打开数据库
    pub fn open(dir: &Path) -> Result<Self, Error> {
        let mut opts = Options::new();
        opts.create_if_missing = true;
        opts.max_open_files = Some(16);
        opts.block_size = Some(8 * 1024 * 1024);
        opts.write_buffer_size = Some(4 * 1024 * 1024);
        opts.cache = Some(Cache::new(1024 * 1024 * 512));
        match Database::open(dir, opts) {
            Ok(db) => Ok(LevelDB { db: db }),
            Err(err) => Error::std(err),
        }
    }
}

#[test]
fn test_leveldb_get_put_del() {
    use crate::block::Block;
    use tempdir::TempDir;
    let tmp = TempDir::new("db").unwrap();
    println!("temp db dir: {:?}", tmp);
    let db = LevelDB::open(tmp.path()).unwrap();
    for i in 1..10u32 {
        let mut blk = Block::default();
        blk.header.set_ver(i as u16);
        db.put(&i.into(), blk, false).unwrap();
    }
    for i in 1..10u32 {
        let blk: Block = db.get(&i.into()).unwrap();
        assert_eq!(i, blk.header.get_ver() as u32);
    }
    for i in 1..5u32 {
        db.del(&i.into()).unwrap();
    }
    for i in 1..5u32 {
        let ret: Option<Block> = db.get(&i.into());
        assert_eq!(None, ret);
    }
}

#[test]
fn test_leveldb_iter() {
    use crate::block::Block;
    use tempdir::TempDir;
    let tmp = TempDir::new("db").unwrap();
    println!("temp db dir: {:?}", tmp);
    let db = LevelDB::open(tmp.path()).unwrap();
    for i in ["1", "123", "1234", "1245", "12345", "2", "3"].iter() {
        let blk = Block::default();
        let k: IKey = i.as_bytes().into();
        db.put(&k.into(), blk, false).unwrap();
    }
    //按前缀迭代
    let prefix: IKey = "2".as_bytes().into();
    let mut iter = db.iter(&prefix);
    while iter.next() {
        let key = iter.key();
        if !key.starts_with(&prefix) {
            break;
        }
        assert_eq!(key, prefix);
    }
    //按前缀迭代
    let prefix: IKey = "123".as_bytes().into();
    let mut iter = db.iter(&prefix);
    let mut i = 0;
    while iter.next() {
        let key = iter.key();
        if !key.starts_with(&prefix) {
            break;
        }
        if i == 0 {
            let ik: IKey = "123".as_bytes().into();
            assert_eq!(key, ik);
        } else if i == 1 {
            let ik: IKey = "1234".as_bytes().into();
            assert_eq!(key, ik);
        } else if i == 2 {
            let ik: IKey = "12345".as_bytes().into();
            assert_eq!(key, ik);
        }
        i += 1;
    }
}
