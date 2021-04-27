use crate::account::Account;
use crate::block::Block;
use crate::errors::Error;
use crate::hasher::Hasher;
use crate::util;
use tempdir::TempDir;

#[derive(Clone)]
pub struct Config {
    /// 数据文件目录
    pub dir: String,
    /// 最低难度
    pub pow_limit: Hasher,
    /// 第一个区块id
    pub genesis: Hasher,
    /// 难度调整周期
    pub pow_time: usize, //14 * 24 * 60 * 60=1209600
    /// 难度区块间隔
    pub pow_span: usize, //2016
    /// 减产配置
    pub halving: usize, //210000
    /// 区块版本
    pub ver: u16,
}

impl Config {
    /// 创建第一个区块
    pub fn create_genesis(&self) -> Result<Block, Error> {
        let mut blk = Block::default();
        blk.header.bits = self.pow_limit.compact();
        blk.header.nonce = util::rand_u32();
        blk.header.set_now_time();
        blk.header.set_ver(self.ver);
        Ok(blk)
    }
    /// 测试用配置
    pub fn test() -> Self {
        let tmp = TempDir::new("btx").unwrap();
        let dir = tmp.path().to_str().unwrap();
        Config {
            ver: 1,
            dir: dir.into(),
            pow_limit: Hasher::must_from(
                "00ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            ),
            genesis: Hasher::must_from(
                "00ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            ),
            pow_time: 14 * 24 * 60 * 60,
            pow_span: 2016,
            halving: 210000,
        }
    }
    /// 发布配置
    pub fn release() -> Self {
        Config {
            ver: 1,
            dir: "/blkdir".into(),
            pow_limit: Hasher::must_from(
                "00000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            ),
            genesis: Hasher::must_from(
                "00ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            ),
            pow_time: 14 * 24 * 60 * 60,
            pow_span: 2016,
            halving: 210000,
        }
    }
}

#[test]
fn test_create_genesis() {
    let blk = Config::test().create_genesis().unwrap();
    println!("{:x?}", blk);
}
