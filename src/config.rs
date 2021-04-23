///配置文件结构定义
///
use crate::hasher::Hasher;
use tempdir::TempDir;

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
}

impl Config {
    /// 测试用配置
    pub fn test() -> Self {
        let tmp = TempDir::new("btx_test_dir").unwrap();
        let dir = tmp.path().to_str().unwrap();
        Config {
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
