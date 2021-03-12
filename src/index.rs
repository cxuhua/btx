use crate::block::{Block, Tx};
use crate::errors::Error;
use crate::iobuf::Serializer;
use core::hash;
use std::cmp::{Eq, PartialEq};

pub struct IKey(Vec<u8>);

impl hash::Hash for IKey {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        state.write(&self.0);
    }
}

impl PartialEq for IKey {
    fn eq(&self, other: &IKey) -> bool {
        self.0 == other.0
    }
}

impl Eq for IKey {}

#[derive(Debug)]
pub enum IValue {
    Block(Block), //存放区块数据
    Tx(Tx),       //存放交易数据
}

impl IValue {
    /// 作为区块返回
    fn as_block(&self) -> Result<&Block, Error> {
        match self {
            IValue::Block(blk) => Ok(&blk),
            _ => Error::msg("not found Block"),
        }
    }
    /// 作为交易返回
    fn as_tx(&self) -> Result<&Tx, Error> {
        match self {
            IValue::Tx(tx) => Ok(&tx),
            _ => Error::msg("not found Tx"),
        }
    }
}

/// 区块链存储索引
pub trait Indexer: Sized {
    /// 根据k获取数据
    fn get(&mut self, _k: &IKey) -> Result<IValue, Error> {
        Error::msg("NotImpErr")
    }
}

/// 数据文件分布说明
/// data  --- 数据根目录
///       --- entry 入口索引文件 leveldb
///       --- block 区块内容目录 store存储
///       --- index 索引目录,金额记录,区块头 leveldb

#[test]
fn test_index() {
    struct A {}
    impl Indexer for A {}
    use lru::LruCache;
    let mut c: LruCache<IKey, IValue> = LruCache::new(100);
}
