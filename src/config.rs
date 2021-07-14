use crate::account::Account;
use crate::block::{Block, Tx, TxIn, TxOut};
use crate::consts;
use crate::errors::Error;
use crate::hasher::Hasher;
use crate::index::Chain;
use crate::script::Script;
use crate::util;
use std::convert::TryInto;
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
    pub pow_time: u32, //14 * 24 * 60 * 60=1209600
    /// 难度区块间隔
    pub pow_span: u32, //2016
    /// 减半配置
    pub halving: u32, //210000
    /// 区块版本
    pub ver: u16,
    /// 默认账户信息
    pub acc: Option<Account>,
}

impl Config {
    /// 计算某高度下可获的奖励
    pub fn compute_reward(&self, h: u32) -> i64 {
        let hlv = h / self.halving;
        if hlv >= 64 {
            return 0;
        }
        let mut n = 50 * consts::COIN;
        n >>= hlv;
        return n;
    }
    /// 创建一个默认配置的区块
    pub fn new_block(&self, p: Hasher, bits: u32) -> Result<Block, Error> {
        let mut blk = Block::default();
        blk.header.bits = bits;
        blk.header.nonce = util::rand_u32();
        blk.header.set_now_time();
        blk.header.set_ver(self.ver);
        blk.header.prev = p;
        Ok(blk)
    }
    /// 创建coinbase交易
    /// h 区块高度
    /// s coinbase自定义数据
    /// acc 输出账号
    pub fn new_coinbase(&self, h: u32, s: &str, acc: &Account) -> Result<Tx, Error> {
        let mut cb = Tx::default();
        cb.ver = 1;
        //交易输入
        let mut inv = TxIn::default();
        inv.out = Hasher::zero();
        inv.idx = 0;
        inv.script = Script::new_script_cb(h, s.as_ref())?;
        inv.seq = 0;
        cb.ins.push(inv);
        //交易输出
        let mut out = TxOut::default();
        out.value = self.compute_reward(h);
        out.script = Script::new_script_out(&acc.hash()?)?;
        cb.outs.push(out);
        Ok(cb)
    }
    /// 创建第一个区块
    /// h 区块高度
    /// s coinbase信息
    /// v 奖励金额
    pub fn create_block<F>(
        &self,
        p: Hasher,
        h: u32,
        bits: u32,
        cbstr: &str,
        ff: F,
    ) -> Result<Block, Error>
    where
        F: FnOnce(&mut Block),
    {
        if self.acc.is_none() {
            return Error::msg("acc option miss");
        }
        let acc = self.acc.as_ref().unwrap();
        let mut blk = self.new_block(p, bits)?;
        //创建coinbase交易
        let cb = self.new_coinbase(h, cbstr, &acc)?;
        blk.append(cb);
        //完成区块时的而外工作
        ff(&mut blk);
        //计算默克尔树
        blk.finish()?;
        //计算工作量
        let mut id = blk.id()?;
        let mut count = 0;
        //验证工作难度
        while !id.verify_pow(&self.pow_limit, blk.header.bits) && count < 1024 * 1024 {
            blk.header.nonce += 1;
            id = blk.id()?;
            count += 1;
        }
        if !id.verify_pow(&self.pow_limit, blk.header.bits) {
            return Error::msg("compute pow failed");
        }
        Ok(blk)
    }
    /// 测试用配置
    /// tf在这个配置上创建链测试方法
    pub fn test<F>(tf: F)
    where
        F: FnOnce(&Config, Chain),
    {
        let tmp = TempDir::new("btx").unwrap();
        let dir = tmp.path().to_str().unwrap();
        //测试账户包含了私钥的默认测试账户
        let acc = Account::decode_from_bech32("aps1qgp07q3pqwm4kyqpxxnknu04xafv7ecwpha9dmwg5ae58lckq6g5a4r3cepx7ggz9y4cgwdyjpgkcvmcd0eaezykj2r5qvzcutpc8gghs6uf5qu5uk7qygxkhplmnz9ymd909v8y0rsk59wlppfjd52hfe47ult5p605zzk6nqsxf8x9n8dwwue7atwxrchana3u6h564l9wqrs3mqsc9ankppwwrve4ljrjgqtdunqzlvjm7lqhz8jgsehtu5gvj0gh9g7ku0x8paejzuqcxgs7");
        let mut conf = Config {
            ver: 1,
            dir: dir.into(),
            pow_limit: "00ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
                .try_into()
                .unwrap(),
            genesis: Hasher::zero(),
            pow_time: 14 * 24 * 60 * 60,
            pow_span: 2016,
            halving: 210000,
            acc: Some(acc.unwrap()),
        };
        //创建第一个区块
        let blk = conf
            .create_block(
                Hasher::zero(),
                0,
                conf.pow_limit.compact(),
                "genesis test block",
                |_| {},
            )
            .unwrap();
        conf.genesis = blk.id().unwrap();
        //打开数据库
        let idx = Chain::new(&conf).unwrap();
        //链接第一个genesis区块
        idx.link(&blk).unwrap();
        tf(&conf, idx);
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
            acc: None,
        }
    }
}

#[test]
fn test_create_genesis() {
    Config::test(|conf, idx| {
        let best = idx.best().unwrap();
        assert_eq!(best.id, conf.genesis);
    });
}

#[test]
fn test_compute_reward() {
    Config::test(|conf, _| {
        let v1 = conf.compute_reward(1);
        assert_eq!(v1, 50 * consts::COIN);
        let v1 = conf.compute_reward(conf.halving as u32);
        assert_eq!(v1, 50 * consts::COIN / 2);
        let v1 = conf.compute_reward((conf.halving * 2) as u32);
        assert_eq!(v1, 50 * consts::COIN / 4);
        let v1 = conf.compute_reward((conf.halving * 3) as u32);
        assert_eq!(v1, 50 * consts::COIN / 8);
    });
}
