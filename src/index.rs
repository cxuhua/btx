use crate::block::{Block, Tx};
use crate::errors::Error;
use crate::hasher::Hasher;

/// 区块链存储索引
pub trait Indexer: Sized {
    /// 从系统根据id查询区块数据
    /// 返回一个借用数据,区块应该使用一个缓存系统进行缓存
    /// 错误一般返回 NotFoundBlock
    fn get_block_from_id(&mut self, _id: &Hasher) -> Result<&Block, Error> {
        Err(Error::NotFoundBlock)
    }
    /// 查询交易数据
    /// 先查询交易所在的区块,然后获取交易信息
    /// 获取指定索引的数据
    /// 错误一般返回NotFoundTx
    fn get_tx_from_id(&mut self, _id: &Hasher, _idx: u16) -> Result<&Tx, Error> {
        Err(Error::NotFoundTx)
    }
    /// 根据高度查询区块数据
    fn get_block_from_height(&mut self, _height: u32) -> Result<&Block, Error> {
        Err(Error::NotFoundBlock)
    }
}

#[test]
fn test_index() {
    struct A {
        blk: Block,
    }
    impl Indexer for A {
        fn get_block_from_height(&mut self, _height: u32) -> Result<&Block, Error> {
            Ok(&self.blk)
        }
    }
    let mut a = A {
        blk: Block::default(),
    };
    let blk = a.get_block_from_height(100).unwrap();
    println!("{:?}", blk);
}
