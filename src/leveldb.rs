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
use std::boxed::Box;
use std::convert::{TryFrom, TryInto};
use std::marker::PhantomData;
use std::path::Path;

/// 批操作对象
/// 写入的数据或者key长度不能超过0xFFFF
pub struct IBatch {
    b: Writebatch<IKey>,
    f: Option<Writebatch<IKey>>,
}

impl Default for IBatch {
    fn default() -> Self {
        IBatch::new(false)
    }
}

/// 最大key和value长度
const MAX_KEY_VALUE: usize = 0xFFFF;

impl IBatch {
    /// 创建批处理对象
    /// rev=true会创建回退日志
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
    pub fn del_bytes(&mut self, k: &IKey, v: Option<&[u8]>) {
        assert!(k.len() < MAX_KEY_VALUE);
        self.b.delete(k.clone());
        if v.is_none() || self.f.is_none() {
            return;
        }
        let fv = &mut self.f.as_mut().unwrap();
        assert!(v.unwrap().len() < MAX_KEY_VALUE);
        fv.put(k.clone(), v.unwrap())
    }
    /// 删除数据并按对象序列写入(如果对象存在)
    pub fn del<V>(&mut self, k: &IKey, v: Option<&V>)
    where
        V: Serializer,
    {
        assert!(k.len() < MAX_KEY_VALUE);
        self.b.delete(k.clone());
        if v.is_none() || self.f.is_none() {
            return;
        }
        let wb = v.unwrap().pack();
        assert!(wb.len() < MAX_KEY_VALUE);
        let fv = &mut self.f.as_mut().unwrap();
        fv.put(k.clone(), wb.bytes());
    }
    /// 添加kv数据
    fn put_bytes(&mut self, k: &IKey, v: &[u8]) {
        assert!(k.len() < MAX_KEY_VALUE && v.len() < MAX_KEY_VALUE);
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
        let wb = v.pack();
        assert!(k.len() < MAX_KEY_VALUE && wb.len() < MAX_KEY_VALUE);
        self.b.put(k.clone(), wb.bytes());
        if let Some(ref mut fv) = self.f {
            fv.delete(k.clone())
        }
    }
    /// 替换数据,同key放入新数据,然后旧的作为回退写入回退数据
    pub fn set<V>(&mut self, k: &IKey, new: &V, old: &V)
    where
        V: Serializer,
    {
        let wb = new.pack();
        assert!(k.len() < MAX_KEY_VALUE && wb.len() < MAX_KEY_VALUE);
        self.b.put(k.clone(), wb.bytes());
        if let Some(ref mut fv) = self.f {
            let wb = old.pack();
            assert!(wb.len() < MAX_KEY_VALUE);
            fv.put(k.clone(), wb.bytes());
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
                    b.del_bytes(&kb.into(), None);
                }
                _ => return Error::msg("type byte error"),
            }
        }
        Ok(b)
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
    /// 写个数据
    pub fn next(&mut self) -> bool {
        self.iter.advance()
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
        while self.next() {
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
    /// 写个数据
    pub fn next(&mut self) -> bool {
        self.iter.advance()
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
        while self.next() {
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

impl LevelDB {
    /// 写入批量数据
    pub fn write(&self, b: &IBatch, sync: bool) -> Result<(), Error> {
        let mut opts = WriteOptions::new();
        opts.sync = sync;
        match self.db.write(opts, &b.b) {
            Ok(_) => Ok(()),
            Err(err) => Error::std(err),
        }
    }
    /// 正向迭代
    pub fn iter<'a>(&'a self, prefix: &'a IKey) -> Iter<'a, Iterator<'a, IKey>> {
        let opts = ReadOptions::new();
        Iter {
            iter: Box::new(self.db.iter(opts).from(prefix)),
            marker: PhantomData,
        }
    }
    /// 反向迭代
    pub fn reverse<'a>(&'a self, key: &'a IKey) -> Iter<'a, RevIterator<'a, IKey>> {
        let opts = ReadOptions::new();
        Iter {
            iter: Box::new(self.db.iter(opts).reverse().from(key)),
            marker: PhantomData,
        }
    }
    /// 添加数据
    pub fn put<V>(&self, k: &IKey, v: &V, sync: bool) -> Result<(), Error>
    where
        V: Serializer,
    {
        let wb = &mut Writer::default();
        v.encode(wb);
        let mut opts = WriteOptions::new();
        opts.sync = sync;
        self.db
            .put(opts, k, wb.bytes())
            .map_or_else(Error::std, |_| Ok(()))
    }
    /// 数据是否存在
    pub fn has(&self, k: &IKey) -> bool {
        let opts = ReadOptions::new();
        self.db
            .get(opts, k)
            .map_or(false, |v| v.map_or(false, |v| v.len() > 0))
    }
    /// 获取数据
    pub fn get<V>(&self, k: &IKey) -> Result<V, Error>
    where
        V: Serializer + Default,
    {
        let opts = ReadOptions::new();
        let buf = self.db.get(opts, k).map_or_else(Error::std, |v| {
            v.map_or(Error::msg("data empty"), |v| Ok(v))
        })?;
        let mut r = Reader::new(&buf);
        r.decode()
    }
    /// 删除数据
    pub fn del(&self, k: &IKey, sync: bool) -> Result<(), Error> {
        let mut opts = WriteOptions::new();
        opts.sync = sync;
        self.db.delete(opts, k).map_or_else(Error::std, |_| Ok(()))
    }
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

struct ReverseComparator<K> {
    marker: PhantomData<K>,
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
        db.put(&i.into(), &blk, false).unwrap();
        assert_eq!(true, db.has(&i.into()));
    }
    assert_eq!(false, db.has(&100.into()));
    for i in 1..10u32 {
        let blk: Block = db.get(&i.into()).unwrap();
        assert_eq!(i, blk.header.get_ver() as u32);
    }
    for i in 1..5u32 {
        db.del(&i.into(), false).unwrap();
    }
    for i in 1..5u32 {
        let ret = db.has(&i.into());
        assert_eq!(false, ret);
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
        db.put(&k.into(), &blk, false).unwrap();
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
        db.put(&k.into(), &blk, false).unwrap();
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
