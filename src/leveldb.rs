use crate::errors::Error;
use crate::index::IKey;
use crate::iobuf::{Reader, Serializer, Writer};
use leveldb::database::batch::{Batch, Writebatch, WritebatchIterator};
use leveldb::database::cache::Cache;
use leveldb::database::Database;
use leveldb::iterator::LevelDBIterator;
use leveldb::iterator::{Iterable, Iterator, RevIterator};
use leveldb::kv::KV;
use leveldb::options::{Options, ReadOptions, WriteOptions};
use std::convert::{TryFrom, TryInto};
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

    /// 是否存在key
    fn has(&self, k: &IKey) -> bool;

    /// KV正向迭代器
    /// 从指定的前缀向后迭代
    fn iter<'a>(&'a self, prefix: &'a IKey) -> Iter<'a, Iterator<'a, IKey>>;

    /// KV反向迭代器
    /// 从指定的key向前迭代
    fn reverse<'a>(&'a self, key: &'a IKey) -> Iter<'a, RevIterator<'a, IKey>>;

    /// 写入批次数据
    fn write(&self, b: &IBatch, sync: bool) -> Result<(), Error>;
}

/// 批操作对象
/// 写入的数据或者key长度不能超过0xFFFF
pub struct IBatch {
    b: Writebatch<IKey>,
    f: Option<Writebatch<IKey>>,
}

impl IBatch {
    /// 创建批处理对象
    pub fn new(rev: bool) -> Self {
        IBatch {
            b: Writebatch::new(),
            f: if rev { Some(Writebatch::new()) } else { None },
        }
    }
    /// 清空批次
    pub fn clear(&mut self) {
        self.b.clear();
        if let Some(ref mut fv) = self.f {
            fv.clear();
        }
    }
    /// 删除数据并写入回退数据(如果需要)
    /// 如果v存在将v写入回退批次
    pub fn del(&mut self, k: &IKey, v: Option<&[u8]>) {
        self.b.delete(k.clone());
        if v.is_none() {
            return;
        }
        if let Some(ref mut fv) = self.f {
            fv.put(k.clone(), v.unwrap())
        }
    }
    /// 添加kv数据
    fn put_bytes(&mut self, k: &IKey, v: &[u8]) {
        assert!(k.len() < 0xFFFF && v.len() < 0xFFFF);
        self.b.put(k.clone(), v);
        if let Some(ref mut fv) = self.f {
            fv.delete(k.clone())
        }
    }
    /// 添加kv数据
    pub fn put<V>(&mut self, k: &IKey, v: &V)
    where
        V: Serializer,
    {
        let mut wb = Writer::default();
        v.encode(&mut wb);
        assert!(k.len() < 0xFFFF && wb.len() < 0xFFFF);
        self.b.put(k.clone(), wb.bytes());
        if let Some(ref mut fv) = self.f {
            fv.delete(k.clone())
        }
    }

    /// 获取批次数据
    pub fn bytes(&mut self) -> Writer {
        let mut writer = Writer::default();
        let iter = IBatchIter { w: &mut writer };
        self.b.iterate(Box::new(iter));
        writer
    }

    /// 获取回退批次写入器
    pub fn reverse(&mut self) -> Writer {
        let mut writer = Writer::default();
        if let Some(ref mut fv) = self.f {
            let iter = IBatchIter { w: &mut writer };
            fv.iterate(Box::new(iter));
        }
        writer
    }
}

#[test]
fn test_leveldb_batch() {
    use crate::block::TxOut;
    //创建写入批次
    let mut b1 = IBatch::new(true);
    for i in 1..10u32 {
        let mut wv = TxOut::default();
        wv.value = i as i64;
        b1.put(&i.into(), &wv);
    }
    let b = &b1.bytes();
    let mut b2: IBatch = b.try_into().unwrap();
    assert_eq!(b1.bytes(), b2.bytes());
    //获取回退批次
    let r = b1.reverse();
    let rb = r.bytes();
    let rw: IBatch = rb.try_into().unwrap();
    //创建测试库
    use tempdir::TempDir;
    let tmp = TempDir::new("db").unwrap();
    let db = LevelDB::open(tmp.path()).unwrap();
    //写入批次
    db.write(&b1, false).unwrap();
    for i in 1..10u32 {
        let out: TxOut = db.get(&i.into()).unwrap();
        assert_eq!(out.value, i as i64);
    }
    //回退批次
    db.write(&rw, false).unwrap();
    for i in 1..10u32 {
        assert_eq!(false, db.has(&i.into()));
    }
    //清空测试
    b1.clear();
    let b = b1.bytes();
    assert_eq!(0, b.len());
}

/// 保存批次数据到Writer
struct IBatchIter<'a> {
    w: &'a mut Writer, //数据长度
}

impl TryFrom<&Writer> for IBatch {
    type Error = Error;
    fn try_from(wb: &Writer) -> Result<Self, Self::Error> {
        wb.bytes().try_into()
    }
}

/// 从字节获取对象
impl TryFrom<&[u8]> for IBatch {
    type Error = Error;
    fn try_from(b: &[u8]) -> Result<Self, Self::Error> {
        let mut r = Reader::new(b);
        let mut b = IBatch::new(false);
        while r.remaining() > 0 {
            match r.u8()? {
                1u8 => {
                    let kl = r.u16()?;
                    let kb = r.get_bytes(kl as usize)?;
                    let vl = r.u16()?;
                    let vv = r.get_bytes(vl as usize)?;
                    b.put_bytes(&kb.into(), &vv);
                }
                2u8 => {
                    let kl = r.u16()?;
                    let kb = r.get_bytes(kl as usize)?;
                    b.del(&kb.into(), None);
                }
                _ => return Error::msg("type byte error"),
            }
        }
        Ok(b)
    }
}

/// key或者value都不应该超过0xFFFF
impl<'a> WritebatchIterator for IBatchIter<'a> {
    type K = IKey;
    /// 添加操作
    fn put(&mut self, k: IKey, v: &[u8]) {
        assert!(k.len() <= 0xFFFF && v.len() <= 0xFFFF);
        //type
        self.w.u8(1);
        //key len
        self.w.u16(k.len() as u16);
        //key
        self.w.put_bytes(k.bytes());
        //value len
        self.w.u16(v.len() as u16);
        //value
        self.w.put_bytes(v);
    }
    /// 删除操作
    fn deleted(&mut self, k: IKey) {
        assert!(k.len() <= 0xFFFF);
        //type
        self.w.u8(2);
        //key len
        self.w.u16(k.len() as u16);
        //key
        self.w.put_bytes(k.bytes());
    }
}

/// 线程安全的kv数据存储实现
pub struct LevelDB {
    db: Database<IKey>,
}

pub struct Iter<'a, T> {
    iter: Box<T>,
    marker: PhantomData<&'a T>,
}

impl<'a> Iter<'a, RevIterator<'a, IKey>> {
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
        if b.len() == 0 {
            return None;
        }
        let mut r = Reader::new(&b);
        r.decode().map_or(None, |v| Some(v))
    }
    /// 按前缀key迭代所有key
    /// 直到结束或者f返回false
    /// f(第几个,当前key,当前value)
    pub fn foreach<F, V>(&mut self, f: F)
    where
        V: Serializer + Default,
        F: Fn(usize, &IKey, &Option<V>) -> bool,
    {
        let mut i = 0;
        while self.iter.advance() {
            if !f(i, &self.key(), &self.value()) {
                break;
            }
            i += 1;
        }
    }
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
        if b.len() == 0 {
            return None;
        }
        let mut r = Reader::new(&b);
        r.decode().map_or(None, |v| Some(v))
    }
    /// 按前缀key迭代所有key
    /// 直到结束或者f返回false
    /// f(第几个,当前key,当前value)
    pub fn foreach<F, V>(&mut self, f: F)
    where
        V: Serializer + Default,
        F: Fn(usize, &IKey, &Option<V>) -> bool,
    {
        let mut i = 0;
        while self.iter.advance() {
            let pkey = self.iter.from_key();
            let key = self.key();
            if !pkey.map_or(false, |v| key.starts_with(v)) {
                break;
            }
            if !f(i, &key, &self.value()) {
                break;
            }
            i += 1;
        }
    }
}

impl DB for LevelDB {
    /// 写入数据库
    fn write(&self, b: &IBatch, sync: bool) -> Result<(), Error> {
        let mut opts = WriteOptions::new();
        opts.sync = sync;
        match self.db.write(opts, &b.b) {
            Ok(_) => Ok(()),
            Err(err) => Error::std(err),
        }
    }
    fn iter<'a>(&'a self, prefix: &'a IKey) -> Iter<'a, Iterator<'a, IKey>> {
        let opts = ReadOptions::new();
        Iter {
            iter: Box::new(self.db.iter(opts).from(prefix)),
            marker: PhantomData,
        }
    }
    fn reverse<'a>(&'a self, key: &'a IKey) -> Iter<'a, RevIterator<'a, IKey>> {
        let opts = ReadOptions::new();
        Iter {
            iter: Box::new(self.db.iter(opts).reverse().from(key)),
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
    fn has(&self, k: &IKey) -> bool {
        let opts = ReadOptions::new();
        match self.db.get(opts, k) {
            Ok(v) => v.map_or(false, |v| v.len() > 0),
            Err(_) => false,
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
    let db = LevelDB::open(tmp.path()).unwrap();
    for i in 1..10u32 {
        let mut blk = Block::default();
        blk.header.set_ver(i as u16);
        db.put(&i.into(), blk, false).unwrap();
        assert_eq!(true, db.has(&i.into()));
    }
    assert_eq!(false, db.has(&100.into()));
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
    let db = LevelDB::open(tmp.path()).unwrap();
    for i in ["1", "123", "1234", "12345", "1245", "2", "3"].iter() {
        let blk = Block::default();
        let k: IKey = i.as_bytes().into();
        db.put(&k.into(), blk, false).unwrap();
    }
    //按前缀迭代
    let prefix: IKey = "2".as_bytes().into();
    let iter = &mut db.iter(&prefix);
    iter.foreach(|_, k, _: &Option<Block>| {
        assert_eq!(k, &prefix);
        true
    });
    //按前缀迭代
    let prefix: IKey = "123".as_bytes().into();
    let iter = &mut db.iter(&prefix);
    let pv = ["123", "1234", "12345"];
    iter.foreach(|i, k, _: &Option<Block>| {
        let ik: IKey = pv[i].into();
        assert_eq!(k, &ik);
        true
    });
}

#[test]
fn test_leveldb_reverse() {
    use crate::block::Block;
    use tempdir::TempDir;
    let tmp = TempDir::new("db").unwrap();
    let db = LevelDB::open(tmp.path()).unwrap();
    for i in ["1", "123", "1234", "12345", "1245", "2", "3"].iter() {
        let blk = Block::default();
        let k: IKey = i.as_bytes().into();
        db.put(&k.into(), blk, false).unwrap();
    }
    //从12345key反向迭代
    let prefix: IKey = "12345".as_bytes().into();
    let iter = &mut db.reverse(&prefix);
    let mut pv = ["1", "123", "1234", "12345"];
    pv.reverse();
    iter.foreach(|i, k, _: &Option<Block>| {
        let ik: IKey = pv[i].into();
        assert_eq!(k, &ik);
        true
    });
}
