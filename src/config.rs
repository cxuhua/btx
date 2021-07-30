use crate::account::HasAddress;
use crate::accpool::AccTestPool;
use crate::errors::Error;
use crate::hasher::Hasher;
use crate::index::Chain;
use std::convert::TryInto;
use tempdir::TempDir;

#[derive(Clone, Debug)]
pub struct Config {
    /// 数据文件目录
    pub dir: String,
    /// 最低难度
    pub pow_limit: Hasher,
    /// 第一个区块id
    pub genesis: Hasher,
    /// 难度调整周期
    pub pow_time: u32, //14 * 24 * 60 * 60=1209600
    /// 难度区块间隔
    pub pow_span: u32, //2016
    /// 减半配置
    pub halving: u32, //210000
    /// 区块版本
    pub ver: u16,
}

impl Config {
    /// 测试用配置
    /// tf在这个配置上创建链测试方法
    pub fn test<F>(tf: F)
    where
        F: FnOnce(&Config, Chain) -> Result<(), Error>,
    {
        let tmp = TempDir::new("btx").unwrap();
        let dir = tmp.path().to_str().unwrap();
        //创建本地账号池
        let accpool = AccTestPool::new();
        //2号账户用来存放区块奖励
        let acc = accpool.value(2).unwrap();
        let addr = acc.string().unwrap();
        let conf = &Config {
            ver: 1,
            dir: dir.into(),
            pow_limit: "00ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
                .try_into()
                .unwrap(),
            genesis: Hasher::zero(),
            pow_time: 14 * 24 * 60 * 60,
            pow_span: 2016,
            halving: 210000,
        };
        let idx = Chain::new(conf, accpool).unwrap();
        //创建第一个区块
        let blk = idx.new_block("genesis block", &addr).unwrap();
        //设置为首个区块
        idx.set_genesis_id(&blk.id().unwrap()).unwrap();
        //链接第一个genesis区块
        idx.link(&blk).unwrap();
        //开始测试
        tf(&idx.config().unwrap(), idx).unwrap();
    }
    /// 发布配置
    pub fn release() -> Self {
        Config {
            ver: 1,
            dir: "/blkdir".into(),
            pow_limit: "00000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
                .try_into()
                .unwrap(),
            genesis: "00ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
                .try_into()
                .unwrap(),
            pow_time: 14 * 24 * 60 * 60,
            pow_span: 2016,
            halving: 210000,
        }
    }
}

#[test]
fn test_create_genesis() {
    Config::test(|conf, idx| {
        let best = idx.best()?;
        assert_eq!(best.id, conf.genesis);
        Ok(())
    });
}

#[test]
fn test_compute_reward() {
    use crate::consts;
    Config::test(|conf, idx| {
        let v1 = idx.compute_reward(1).unwrap();
        assert_eq!(v1, 50 * consts::COIN);
        let v1 = idx.compute_reward(conf.halving as u32).unwrap();
        assert_eq!(v1, 50 * consts::COIN / 2);
        let v1 = idx.compute_reward((conf.halving * 2) as u32).unwrap();
        assert_eq!(v1, 50 * consts::COIN / 4);
        let v1 = idx.compute_reward((conf.halving * 3) as u32).unwrap();
        assert_eq!(v1, 50 * consts::COIN / 8);
        Ok(())
    });
}
