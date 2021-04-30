use crate::account::Account;
use crate::block::{Block, Checker, Tx, TxIn, TxOut};
use crate::consts;
use crate::errors::Error;
use crate::hasher::Hasher;
use crate::script::Script;
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
    /// 减半配置
    pub halving: usize, //210000
    /// 区块版本
    pub ver: u16,
    /// 默认账户信息
    pub acc: Option<Account>,
}

impl Config {
    /// 创建第一个区块
    /// h 区块高度
    /// s coinbase信息
    /// v 奖励金额
    pub fn create_block(&self, h: u32, s: &str, c: i64) -> Result<Block, Error> {
        if self.acc.is_none() {
            return Error::msg("acc option miss");
        }
        let acc = self.acc.as_ref().unwrap();
        let mut blk = Block::default();
        //设置区块头
        blk.header.bits = self.pow_limit.compact();
        blk.header.nonce = util::rand_u32();
        blk.header.set_now_time();
        blk.header.set_ver(self.ver);
        //创建coinbase交易
        let mut cb = Tx::default();
        cb.ver = 1;
        let mut inv = TxIn::default();
        inv.out = Hasher::zero();
        inv.idx = 0;
        inv.script = Script::new_script_cb(h, s.as_ref())?;
        inv.seq = 0;
        cb.ins.push(inv);
        let mut out = TxOut::default();
        out.value = c;
        out.script = Script::new_script_out(&acc.hash()?)?;
        cb.outs.push(out);
        blk.append(cb);

        blk.header.merkle = blk.compute_merkle()?;

        loop {
            let id = blk.id()?;
            if !id.verify_pow(&self.pow_limit, blk.header.bits) {
                blk.header.nonce += 1;
                continue;
            }
            break;
        }
        Ok(blk)
    }
    /// 测试用配置
    pub fn test() -> Self {
        let tmp = TempDir::new("btx").unwrap();
        let dir = tmp.path().to_str().unwrap();
        //测试账户包含了私钥的默认测试账户
        let acc = Account::decode_from_hex("0202ff0221037b9a5dd166a3ee3870716c38d71db913e007fd278c83ada200caafb7c10402d72103664433bfea56f8c8c173b98a70ab0412d9b9bb5c1ed64b6a18778dd111cf1eed02208ca63f306cc974393f5f463eef94c22217c70fea913037d7ccee7728ac0598c4207fdd7ae29bd80594754cfd97d32c59e8d402ed70b372fb4a6d01d1609138d2b6");
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
            acc: Some(acc.unwrap()),
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
            acc: None,
        }
    }
}

#[test]
fn test_create_genesis() {
    let blk = Config::test()
        .create_block(0, "f1", consts::coin(50))
        .unwrap();
    println!("{:x?}", blk);
}
