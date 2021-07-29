use crate::account::{Account, AccountPool, HasAddress};
use crate::block::{Best, BlkAttr, Block, Checker, Tx, TxAttr, TxIn, TxOut};
use crate::bytes::IntoBytes;
use crate::config::Config;
use crate::consts;
use crate::errors::Error;
use crate::hasher::Hasher;
use crate::iobuf::Writer;
use crate::iobuf::{Reader, Serializer};
use crate::leveldb::{IBatch, LevelDB};
use crate::script::{Ele, Exector, ExectorEnv, Script};
use crate::store::Store;
use crate::util;
use bytes::BufMut;
use core::hash;
use db_key::Key;
use lru::LruCache;
use sha2::digest::BlockInput;
use std::cmp::{Eq, PartialEq};
use std::collections::btree_map;
use std::collections::{BTreeMap, HashMap};
use std::convert::{Into, TryFrom, TryInto};
use std::iter::Rev;
use std::path::Path;
use std::slice;
use std::sync::Arc;
use std::sync::RwLock;

/// 交易助手输出元素
#[derive(Debug, Clone)]
pub struct TxOutEle {
    value: i64,   //输出金额
    addr: String, //输出地址
}

impl TxOutEle {
    pub fn new(addr: &str, coin: i64) -> Self {
        TxOutEle {
            value: coin,
            addr: addr.into(),
        }
    }
}

/// 交易助手
/// 生成交易信息
pub struct TxHelper<'a> {
    ver: u32,                          //交易版本
    coins: Vec<CoinAttr>,              //使用的金额作为输入信息
    outs: Vec<TxOutEle>,               //输出金额
    tfee: i64,                         //交易费
    ctx: &'a Chain,                    //链对象
    kaddr: Option<Hasher>,             //找零地址
    signer: Option<Box<dyn TxSigner>>, //签名器,如果设置执行签名
}

impl<'a> TryFrom<&TxHelper<'a>> for Tx {
    type Error = Error;
    fn try_from(helper: &TxHelper<'a>) -> Result<Self, Self::Error> {
        if !consts::is_valid_amount(helper.tfee) {
            return Error::msg("tfee error");
        }
        let mut kaddr: Option<Hasher> = helper.kaddr.clone();
        let (mut ifee, mut ofee) = (0, 0);
        let mut tx = Tx::default();
        tx.ver = helper.ver;
        //获取账户池
        let accpool = helper.ctx.get_account_pool()?;
        for coin in helper.coins.iter() {
            //金额对应的账户信息,如果没有将不能消费这个金额
            let acc = accpool.account(&coin.cpk.string()?)?;
            let mut inv = TxIn::default();
            inv.out = coin.tx.clone();
            inv.idx = coin.idx;
            //未签名脚本
            inv.script = Script::new_script_in(&acc)?;
            inv.seq = 0;
            tx.ins.push(inv);
            ifee += coin.value;
            //如果未设置找零输出账户使用第一个
            if kaddr.is_none() {
                kaddr = Some(acc.get_address()?);
            }
        }
        for ele in helper.outs.iter() {
            //不允许金额为0的输出
            if ele.value == 0 {
                continue;
            }
            let addr = Account::decode(&ele.addr)?;
            let mut outv = TxOut::default();
            outv.value = ele.value;
            outv.script = Script::new_script_out(&addr)?;
            tx.outs.push(outv);
            ofee += ele.value
        }
        if !consts::is_valid_amount(ofee) || !consts::is_valid_amount(ifee) {
            return Error::msg("fee error");
        }
        if ofee > ifee {
            return Error::msg("ofee > ifee error");
        }
        //剩余的金额转到找零地址
        let kfee = ifee - ofee - helper.tfee;
        if !consts::is_valid_amount(kfee) {
            return Error::msg("kfee error");
        }
        if kfee > 0 && kaddr.is_none() {
            return Error::msg("kaddr account miss");
        }
        if kfee > 0 {
            let mut outk = TxOut::default();
            outk.value = kfee;
            outk.script = Script::new_script_out(&kaddr.unwrap())?;
            tx.outs.push(outk);
        }
        //如果设置签名处理器立即签名交易
        if let Some(ref signer) = helper.signer {
            signer.sign_tx(helper.ctx, &mut tx)?;
        }
        Ok(tx)
    }
}

impl<'a> TxHelper<'a> {
    /// 设置交易版本
    pub fn set_signer<S>(&mut self, signer: S) -> Result<&mut Self, Error>
    where
        S: TxSigner + 'static,
    {
        self.signer = Some(Box::new(signer));
        Ok(self)
    }
    /// 设置找零账户hasher
    pub fn set_keep_addr(&mut self, addr: &str) -> Result<&mut Self, Error> {
        let kaddr = Account::decode(addr)?;
        self.kaddr = Some(kaddr.clone());
        Ok(self)
    }
    /// 设置交易费
    pub fn set_cost_fee(&mut self, fee: i64) -> Result<&mut Self, Error> {
        self.tfee = fee;
        Ok(self)
    }
    /// 设置输出
    pub fn set_outs(&mut self, eles: &Vec<TxOutEle>) -> Result<&mut Self, Error> {
        self.outs = eles.clone();
        Ok(self)
    }
    /// 添加输出
    pub fn add_out(&mut self, addr: &str, coin: i64) -> Result<&mut Self, Error> {
        self.outs.push(TxOutEle {
            addr: addr.into(),
            value: coin,
        });
        Ok(self)
    }
    /// 设置使用的金额
    pub fn set_coins(&mut self, coins: &Vec<CoinAttr>) -> Result<&mut Self, Error> {
        self.coins = coins.clone();
        Ok(self)
    }
    /// 添加金额
    pub fn add_coin(&mut self, coin: &CoinAttr) -> Result<&mut Self, Error> {
        self.coins.push(coin.clone());
        Ok(self)
    }
    /// 设置交易版本
    pub fn set_ver(&mut self, ver: u32) -> Result<&mut Self, Error> {
        self.ver = ver;
        Ok(self)
    }
    pub fn new(ctx: &'a Chain) -> Self {
        TxHelper {
            ver: 1,
            coins: vec![],
            outs: vec![],
            tfee: 0,
            ctx: ctx,
            kaddr: None,
            signer: None,
        }
    }
}

/// 交易池,存储将要进入区块的有效交易
pub struct TxPool {
    byid: HashMap<IKey, Arc<Tx>>,       //按交易id存储
    byfee: BTreeMap<i64, Vec<Arc<Tx>>>, //按交易金额排序
}

impl Default for TxPool {
    fn default() -> Self {
        TxPool {
            byid: HashMap::<IKey, Arc<Tx>>::default(),
            byfee: BTreeMap::<i64, Vec<Arc<Tx>>>::default(),
        }
    }
}

/// 交易池按价格迭代器
pub struct TxPoolIter<'a> {
    pool: &'a TxPool,
    rev: Rev<btree_map::Iter<'a, i64, Vec<Arc<Tx>>>>,
    iter: Option<slice::Iter<'a, Arc<Tx>>>,
}

impl<'a> Iterator for TxPoolIter<'a> {
    type Item = Arc<Tx>;
    fn next(&mut self) -> Option<Self::Item> {
        match self.iter {
            Some(ref mut iter) => match iter.next() {
                Some(next) => {
                    return Some(next.clone());
                }
                None => {
                    self.iter = None;
                    return self.next();
                }
            },
            None => match self.rev.next() {
                Some(v) => {
                    self.iter = Some(v.1.iter());
                    return self.next();
                }
                None => {
                    return None;
                }
            },
        }
    }
}

impl TxPool {
    /// 交易池按交易费从大到小获取
    pub fn iter<'a>(&'a mut self) -> TxPoolIter<'a> {
        TxPoolIter {
            pool: self,
            rev: self.byfee.iter().rev(),
            iter: None,
        }
    }
    /// 检测交易是否可进行交易池
    fn check_value(&self, tx: &Tx) -> Result<(), Error> {
        //必须有输入和输出
        if tx.ins.len() == 0 || tx.outs.len() == 0 {
            return Error::msg("ins or outs empy");
        }
        //交易池中不应该有coinbase交易
        if tx.is_coinbase() {
            return Error::msg("txpool don't have coinbase tx");
        }
        let ref key: IKey = tx.id()?.as_ref().into();
        //交易是否已经存在
        if self.byid.contains_key(key) {
            return Error::msg("tx exists");
        }
        for inv in tx.ins.iter() {
            //引用的交易已经存在交易池中消耗
            if self.is_cost_coin(&inv.out, inv.idx) {
                return Error::msg("ref out is cost");
            }
        }
        //检测输出金额是否正确
        let mut ofee: i64 = 0;
        for outv in tx.outs.iter() {
            ofee += outv.value;
        }
        if !consts::is_valid_amount(ofee) {
            return Error::msg("out fee error");
        }
        Ok(())
    }
    /// 添加交易,添加前需要检测是否合法
    /// fee要先计算出来
    /// 返回交易id
    pub fn push(&mut self, tx: &Tx, fee: i64) -> Result<Hasher, Error> {
        //检测是否可进入交易池
        self.check_value(tx)?;
        let id = tx.id()?;
        let ref key: IKey = id.as_ref().into();
        let rtx = Arc::new(tx.clone());
        self.byid.insert(key.clone(), rtx.clone());
        //如果已经存在追加到数组中
        match self.byfee.get_mut(&fee) {
            Some(ref mut fees) => {
                fees.push(rtx.clone());
            }
            None => {
                self.byfee.insert(fee, vec![rtx.clone()]);
            }
        }
        Ok(id)
    }
    /// 按交易id移除成功返回交易信息
    pub fn remove(&mut self, id: &Hasher) -> Result<Arc<Tx>, Error> {
        let ref key: IKey = id.as_ref().into();
        match self.byid.remove(key) {
            Some(tx) => {
                //从金额排序队列删除
                for vs in self.byfee.iter_mut() {
                    vs.1.retain(|vtx| {
                        if let Ok(ref tmp) = vtx.id() {
                            tmp == id
                        } else {
                            false
                        }
                    });
                }
                Ok(tx.clone())
            }
            None => Error::msg("id tx miss"),
        }
    }
    /// 获取交易池长度
    pub fn len(&self) -> usize {
        self.byid.len()
    }
    /// 交易id和idx对应的输出是否再交易池中被消费
    /// 区块链接的时候虽然消费的coin存在,单如果已经再交易池被消费,也不能进去区块
    pub fn is_cost_coin(&self, id: &Hasher, idx: u16) -> bool {
        for tx in self.byid.iter() {
            for inv in tx.1.ins.iter() {
                if &inv.out == id && inv.idx == idx {
                    return true;
                }
            }
        }
        false
    }
    /// 获取账户输出金额
    /// 来自交易池的金额是不能直接消费的
    pub fn coins(&self, acc: &Account) -> Result<Vec<CoinAttr>, Error> {
        let addr = acc.get_address()?;
        let mut coins = vec![];
        for tx in self.byid.iter() {
            let id = tx.1.id()?;
            for (idx, outv) in tx.1.outs.iter().enumerate() {
                let mut coin = CoinAttr::default();
                //只获取属于acc的金额
                coin.cpk = outv.get_address()?;
                if coin.cpk != addr {
                    continue;
                }
                coin.tx = id.clone();
                coin.idx = idx as u16;
                coin.value = outv.value;
                coin.flags = COIN_ATTR_FLAGS_TXPOOL;
                coin.height = u32::MAX; //内存池交易不存在高度
                coins.push(coin);
            }
        }
        Ok(coins)
    }
    /// 获取金额信息
    /// acc: 账户hash id
    /// tx:交易hash id
    /// idx:输出未知
    pub fn get_coin(&self, acc: &Hasher, id: &Hasher, idx: u16) -> Result<CoinAttr, Error> {
        let ref key: IKey = id.as_ref().into();
        let tx = self
            .byid
            .get(key)
            .map_or(Error::msg("tx miss"), |v| Ok(v))?;
        let outv = tx.get_out(idx as usize)?;
        if &outv.get_address()? != acc {
            return Error::msg("acc outv miss");
        }
        let mut coin = CoinAttr::default();
        coin.cpk = acc.clone();
        coin.tx = id.clone();
        coin.idx = idx;
        coin.value = outv.value;
        coin.flags = COIN_ATTR_FLAGS_TXPOOL;
        coin.height = u32::MAX; //内存池交易不存在高度
        Ok(coin)
    }
}

#[derive(PartialEq, Eq, Clone, PartialOrd, Ord, Debug)]
pub struct IKey(Vec<u8>);

/// 自身包含key的兑现无需指定写入key
pub trait HasKey {
    //返回对象的key
    fn key(&self) -> IKey;
}

///
impl Key for IKey {
    fn from_u8(key: &[u8]) -> IKey {
        IKey(key.to_vec())
    }
    fn as_slice<T, F: Fn(&[u8]) -> T>(&self, f: F) -> T {
        f(self.bytes())
    }
}

impl From<u32> for IKey {
    fn from(v: u32) -> Self {
        let mut key = IKey(vec![]);
        key.0.put_u32_le(v);
        key
    }
}

impl From<&[u8]> for IKey {
    fn from(v: &[u8]) -> Self {
        IKey(v.to_vec())
    }
}

impl From<&str> for IKey {
    fn from(v: &str) -> Self {
        IKey(v.as_bytes().to_vec())
    }
}

///
impl From<Vec<u8>> for IKey {
    fn from(v: Vec<u8>) -> Self {
        IKey(v)
    }
}

/// 按hash查询
impl From<&Hasher> for IKey {
    fn from(v: &Hasher) -> Self {
        IKey(v.into_bytes())
    }
}

impl hash::Hash for IKey {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        state.write(&self.0);
    }
}

impl IKey {
    //是否是高度key
    pub fn is_height_key(&self) -> bool {
        self.len() == 4
    }
    /// key 长度
    pub fn len(&self) -> usize {
        self.0.len()
    }
    /// 获取key字节
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
    /// 连接字节到key
    pub fn concat(&mut self, v: &[u8]) -> &mut Self {
        self.0.put_slice(v);
        self
    }
    /// 空key
    pub fn empty() -> Self {
        "".as_bytes().into()
    }
    /// 是否包含前缀
    pub fn starts_with(&self, prefix: &IKey) -> bool {
        self.0.starts_with(&prefix.0)
    }
}

/// 是否来自coinbase交易
const COIN_ATTR_FLAGS_COINBASE: u8 = 1 << 0;
/// 是否来自交易池
const COIN_ATTR_FLAGS_TXPOOL: u8 = 1 << 1;

/// 金额数据结构
/// 地址对应的金额将存储在key方法返回的key中
/// cpk tx idx 使用存储key去填充
#[derive(Clone, Debug)]
pub struct CoinAttr {
    cpk: Hasher, //拥有此金额的公钥hasher
    tx: Hasher,  //所在交易
    idx: u16,    //所在交易的输出
    value: i64,  //金额
    flags: u8,   //是否在coinbase交易
    height: u32, //所在区块高度
}

impl HasAddress for CoinAttr {
    //cpk就是地址hasher直接返回
    fn get_address(&self) -> Result<Hasher, Error> {
        Ok(self.cpk.clone())
    }
}

impl Serializer for CoinAttr {
    /// 编码数据到writer
    fn encode(&self, w: &mut Writer) {
        w.i64(self.value);
        w.u8(self.flags);
        w.u32(self.height);
    }
    /// 从reader读取数据
    fn decode(r: &mut Reader) -> Result<Self, Error>
    where
        Self: Default,
    {
        let mut coin = Self::default();
        coin.value = r.i64()?;
        coin.flags = r.u8()?;
        coin.height = r.u32()?;
        Ok(coin)
    }
}

impl Default for CoinAttr {
    fn default() -> Self {
        CoinAttr {
            cpk: Hasher::zero(),
            tx: Hasher::zero(),
            idx: 0,
            value: 0,
            flags: 0,
            height: 0,
        }
    }
}

/// coin可以作为兑现直接按次key存储
impl HasKey for CoinAttr {
    /// 获取存储key
    fn key(&self) -> IKey {
        let mut w = Writer::default();
        w.encode(&self.cpk);
        w.encode(&self.tx);
        w.u16(self.idx);
        IKey::from_u8(w.bytes())
    }
}

impl CoinAttr {
    /// 获取金额
    #[inline]
    pub fn coin(&self) -> i64 {
        self.value
    }
    /// 是否来自交易池
    pub fn is_txpool(&self) -> bool {
        self.flags & COIN_ATTR_FLAGS_TXPOOL != 0
    }
    /// 是否来自coinbase交易
    pub fn is_coinbase(&self) -> bool {
        self.flags & COIN_ATTR_FLAGS_COINBASE != 0
    }
    /// 金额在spent高度上是否有效
    pub fn is_valid(&self, spent: u32) -> bool {
        //来自交易池不可用
        if self.is_txpool() {
            return false;
        }
        //非coinbase可用
        if !self.is_coinbase() {
            return true;
        }
        //coinbase输出必须在100个高度后才可消费
        return spent - self.height >= consts::COINBASE_MATURITY;
    }
    /// 从存储key获取
    pub fn from_key(k: &IKey) -> Result<Self, Error> {
        let mut r = Reader::new(k.bytes());
        let mut v = Self::default();
        v.cpk = r.decode()?;
        v.tx = r.decode()?;
        v.idx = r.u16()?;
        Ok(v)
    }
    /// 从存储key获取
    pub fn fill_key(&mut self, k: &IKey) -> Result<&Self, Error> {
        let mut r = Reader::new(k.bytes());
        self.cpk = r.decode()?;
        self.tx = r.decode()?;
        self.idx = r.u16()?;
        Ok(self)
    }
    /// 填充值
    pub fn fill_value_with_reader(&mut self, r: &mut Reader) -> Result<&mut Self, Error> {
        self.value = r.i64()?;
        self.flags = r.u8()?;
        self.height = r.u32()?;
        Ok(self)
    }
    /// 填充值
    pub fn fill_value(&mut self, bytes: &[u8]) -> Result<&mut Self, Error> {
        self.fill_value_with_reader(&mut Reader::new(bytes))
    }
    /// 获取值
    pub fn value(&self) -> Writer {
        let mut w = Writer::default();
        self.encode(&mut w);
        w
    }
}

#[test]
fn test_coin_attr_key_value() {
    use std::convert::TryFrom;
    let mut attr = CoinAttr::default();
    attr.cpk = Hasher::try_from("12345678ffffffffffffffffffffffffffffffffffffffffffffffff12345678")
        .unwrap();
    attr.tx = Hasher::try_from("87654321ffffffffffffffffffffffffffffffffffffffffffffffff87654321")
        .unwrap();
    attr.idx = 0x9988;
    attr.value = 78965;
    attr.flags = 1;
    attr.height = 2678;
    let value = attr.value();
    let mut attr2 = CoinAttr::from_key(&attr.key()).unwrap();
    let attr2 = attr2.fill_value(value.bytes()).unwrap();
    assert_eq!(attr.cpk, attr2.cpk);
    assert_eq!(attr.tx, attr2.tx);
    assert_eq!(attr.idx, attr2.idx);
    assert_eq!(attr.value, attr2.value);
    assert_eq!(attr.flags, attr2.flags);
    assert_eq!(attr.height, attr2.height);
}

#[test]
fn test_block_index_coin_save() {
    Config::test(|conf, idx| {
        let acc = conf.acc.as_ref().unwrap();
        //只加入的genesis区块
        let best = idx.best()?;
        assert_eq!(best.id, conf.genesis);
        let coins = idx.coins(&acc)?;
        assert_eq!(1, coins.len());
        assert_eq!(coins[0].cpk, acc.hash()?);
        assert_eq!(coins[0].value, consts::coin(50));
        assert_eq!(coins[0].flags, 1);
        assert_eq!(coins[0].height, 0);
        //链接一个新的区块
        let blk = idx.new_block("second", |_| Ok(()))?;
        let best = idx.link(&blk)?;
        assert_eq!(best.height, 1);
        let coins = idx.coins(&acc)?;
        assert_eq!(2, coins.len());
        assert_eq!(coins[0].height, 0);
        assert_eq!(coins[1].cpk, acc.hash()?);
        assert_eq!(coins[1].value, consts::coin(50));
        assert_eq!(coins[1].flags, 1);
        assert_eq!(coins[1].height, 1);
        let pop = idx.pop()?;
        assert_eq!(pop.id()?, blk.id()?);
        //弹出一个只剩下genesis区块了
        let best = idx.best()?;
        assert_eq!(best.id, conf.genesis);
        let coins = idx.coins(&acc)?;
        assert_eq!(1, coins.len());
        assert_eq!(coins[0].cpk, acc.hash()?);
        assert_eq!(coins[0].value, consts::coin(50));
        assert_eq!(coins[0].flags, 1);
        assert_eq!(coins[0].height, 0);
        assert_eq!(idx.pop().is_err(), true);
        Ok(())
    });
}

/// 区块链数据存储索引
pub struct BlkIndexer {
    cache: BlkCache,                   //缓存
    leveldb: LevelDB,                  //索引数据库指针
    blk: Store,                        //区块存储
    rev: Store,                        //回退日志存储
    conf: Config,                      //配置信息
    pool: TxPool,                      //交易内存池,获取到的新交易存在,按交易费从高到低存放
    acp: Option<Arc<dyn AccountPool>>, //账户池
}

/// 签名验证数据缓存
pub struct LinkExectorCache {
    pub ver: u32,     //交易版本
    pub outs: Hasher, //输出hash缓存 tx.outs
    pub refs: Hasher, //引用hash缓存 tx.ins.out tx.ins.idx
}

impl LinkExectorCache {
    /// 创建缓存
    fn new(tx: &Tx) -> Result<Self, Error> {
        let mut outs = Writer::default();
        for outv in tx.outs.iter() {
            outv.encode_sign(&mut outs)?;
        }
        let mut refs = Writer::default();
        for inv in tx.ins.iter() {
            refs.encode(&inv.out);
            refs.u16(inv.idx);
        }
        Ok(LinkExectorCache {
            ver: tx.ver,
            outs: Hasher::hash(outs.bytes()),
            refs: Hasher::hash(refs.bytes()),
        })
    }
}

/// 交易签名器
pub trait TxSigner {
    /// 获取签名缓存器
    fn get_sign_cache(&self, tx: &Tx) -> Result<LinkExectorCache, Error> {
        LinkExectorCache::new(tx)
    }
    /// 获取签名脚本
    /// inv:当前输入
    /// outv:当前输入引用的输出
    fn get_sign_script(
        &self,
        cache: &LinkExectorCache,
        inv: &TxIn,
        outv: &TxOut,
        accpool: Arc<dyn AccountPool>,
    ) -> Result<Script, Error>;
    /// 获取签名数据
    /// inv:当前输入
    /// outv:当前输入引用的输出
    fn get_sign_bytes(
        &self,
        cache: &LinkExectorCache,
        inv: &TxIn,
        outv: &TxOut,
    ) -> Result<Writer, Error> {
        let mut w = Writer::default();
        w.u32(cache.ver); //版本
        w.encode(&cache.refs); //引用hash
        inv.encode_sign(&mut w)?; //输入
        outv.encode_sign(&mut w)?; //引用的输出
        w.encode(&cache.outs); //输出hash
        Ok(w)
    }
    /// 签名交易
    fn sign_tx(&self, ctx: &Chain, tx: &mut Tx) -> Result<(), Error> {
        let accpool = ctx.get_account_pool()?;
        let cache = self.get_sign_cache(tx)?;
        for inv in tx.ins.iter_mut() {
            let outv = ctx.get_txin_ref_txout(&inv)?;
            inv.script = self.get_sign_script(&cache, &inv, &outv, accpool.clone())?
        }
        Ok(())
    }
    /// 验签,acc需要包含公钥
    fn verify_tx(
        &self,
        cache: &LinkExectorCache,
        inv: &TxIn,
        outv: &TxOut,
        acc: &Account,
    ) -> Result<bool, Error>;
}

/// 全签名验签处理
pub struct FullSigner {}

//根据当前交易获取签名验签
pub fn new_tx_signer(_ctx: &mut BlkIndexer, _tx: &Tx) -> Box<dyn TxSigner> {
    Box::new(FullSigner {})
}

impl TxSigner for FullSigner {
    /// 获取带签名的输入脚本
    fn get_sign_script(
        &self,
        cache: &LinkExectorCache,
        inv: &TxIn,
        outv: &TxOut,
        accpool: Arc<dyn AccountPool>,
    ) -> Result<Script, Error> {
        //输入应该已经设置消费账户
        let addr = inv.string()?;
        //获取输入对应的账户
        let acc = accpool.account(&addr)?;
        if !acc.check_pris_pubs() {
            return Error::msg("acc miss private key");
        }
        let mut acc = (*acc).clone();
        //获取签名数据
        let msg = self.get_sign_bytes(cache, inv, outv)?;
        //全部签名
        acc.sign_full(msg.bytes())?;
        //返回包含签名的脚本
        Script::new_script_in(&acc)
    }
    /// 验签,acc需要包含公钥
    fn verify_tx(
        &self,
        cache: &LinkExectorCache,
        inv: &TxIn,
        outv: &TxOut,
        acc: &Account,
    ) -> Result<bool, Error> {
        let msg = self.get_sign_bytes(cache, inv, outv)?;
        acc.verify_full(msg.bytes())
    }
}

/// 区块链接脚本执行签名验证
struct LinkExectorEnv<'a> {
    tx: &'a Tx,                    //当前交易
    inv: &'a TxIn,                 //当前输入
    outv: &'a TxOut,               //输入引用的输出
    cache: &'a LinkExectorCache,   //执行交易缓存
    singer: Box<&'a dyn TxSigner>, //签名验签
}

impl<'a> ExectorEnv for LinkExectorEnv<'a> {
    fn verify_sign(&self, ele: &Ele) -> Result<bool, Error> {
        let acc: Account = ele.try_into()?;
        self.singer.verify_tx(self.cache, self.inv, self.outv, &acc)
    }
}

/// 数据文件分布说明
/// data  --- 数据根目录
///       --- block 区块内容目录 store存储
///       --- index 索引目录,金额记录,区块头 leveldb
impl BlkIndexer {
    /// 获取账户池
    pub fn get_account_pool(&self) -> Result<Arc<dyn AccountPool>, Error> {
        match &self.acp {
            Some(acp) => Ok(acp.clone()),
            None => Error::msg("acc pool miss"),
        }
    }
    /// 设置账户池对象
    pub fn set_account_pool(&mut self, acp: Arc<dyn AccountPool>) -> Result<(), Error> {
        self.acp = Some(acp);
        Ok(())
    }
    /// 创建下个区块
    /// 保证第一个区块已经链接到链
    /// cbstr: coinbase区块自定义信息
    pub fn new_block<F>(&mut self, cbstr: &str, ff: F) -> Result<Block, Error>
    where
        F: FnOnce(&mut Block) -> Result<(), Error>,
    {
        let best = self.best()?;
        let height = best.next();
        let bits = self.next_bits()?;
        self.conf.create_block(best.id, height, bits, cbstr, ff)
    }
    /// 每个文件最大大小
    const MAX_FILE_SIZE: u32 = 1024 * 1024 * 512;
    /// 最高区块入口存储key
    const BEST_KEY: &'static str = "__best__key__";
    /// 创建区块索引存储对象
    fn new(conf: &Config) -> Result<Self, Error> {
        //根目录
        let dir = &conf.dir;
        util::miss_create_dir(dir)?;
        //索引目录
        let idxdir = String::from(dir) + "/index";
        util::miss_create_dir(&idxdir)?;
        //区块目录
        let blkdir = String::from(dir) + "/block";
        util::miss_create_dir(&blkdir)?;
        Ok(BlkIndexer {
            cache: BlkCache::default(),
            leveldb: LevelDB::open(Path::new(&idxdir))?,
            blk: Store::new(&blkdir, "blk", Self::MAX_FILE_SIZE)?,
            rev: Store::new(&blkdir, "rev", Self::MAX_FILE_SIZE)?,
            conf: conf.clone(),
            pool: TxPool::default(),
            acp: None,
        })
    }
    /// 添加交易到交易池
    pub fn append(&mut self, tx: &Tx) -> Result<Hasher, Error> {
        //检测基本值
        tx.check_value(self)?;
        let best = self.best()?;
        //检测签名
        self.check_tx_sign(tx)?;
        //检测下个高度下金额是否正确
        self.check_tx_amount(best.next(), &tx)?;
        let id = tx.id()?;
        let fee = self.get_tx_transaction_fee(tx)?;
        self.pool.push(tx, fee)?;
        Ok(id)
    }
    /// 从交易池移除交易
    pub fn remove(&mut self, id: &Hasher) -> Result<Arc<Tx>, Error> {
        self.pool.remove(id)
    }
    /// 获取当前配置
    pub fn config(&self) -> &Config {
        &self.conf
    }
    /// 获取最高区块信息
    /// 不存在应该是没有区块记录
    pub fn best(&self) -> Result<Best, Error> {
        let key: IKey = Self::BEST_KEY.into();
        self.leveldb.get(&key)
    }
    /// 下个区块的高度
    pub fn next_height(&self) -> u32 {
        self.best().map_or(0, |v| v.height + 1)
    }
    /// 获取下个区块难度
    pub fn next_bits(&mut self) -> Result<u32, Error> {
        let best = self.best();
        if best.is_err() {
            return Ok(self.conf.pow_limit.compact());
        }
        let best = best.unwrap();
        if best.height == 0 {
            return Ok(self.conf.pow_limit.compact());
        }
        let last: BlkAttr = self.attr(&best.id.as_ref().into())?;
        let next = best.next();
        if next % self.conf.pow_span != 0 {
            return Ok(last.bhv.bits);
        }
        let prev = next - self.conf.pow_span;
        let prev: BlkAttr = self.attr(&prev.into())?;
        let ct = last.bhv.get_timestamp();
        let pt = prev.bhv.get_timestamp();
        Ok(self
            .conf
            .pow_limit
            .compute_bits(self.conf.pow_time, ct, pt, last.bhv.bits))
    }
    /// 获取账户对应的金额列表
    pub fn coins(&self, acc: &Account) -> Result<Vec<CoinAttr>, Error> {
        let mut coins: Vec<CoinAttr> = vec![];
        let hash = acc.get_address()?;
        let akey: IKey = hash.as_ref().into();
        let iter = &mut self.leveldb.iter(&akey);
        while iter.next() {
            let key = &iter.key();
            if !key.starts_with(&akey) {
                break;
            }
            let value: Option<CoinAttr> = iter.value();
            if value.is_none() {
                return Error::msg("coin value none");
            }
            let mut value = value.unwrap();
            value.fill_key(key)?;
            coins.push(value);
        }
        //按所在区块高度从小到大排序
        coins.sort_by(|a, b| a.height.cmp(&b.height));
        //加入交易池中的金额
        let mut pcoins = self.pool.coins(acc)?;
        coins.append(&mut pcoins);
        Ok(coins)
    }
    /// 获取属性信息
    pub fn attr<T>(&self, k: &IKey) -> Result<T, Error>
    where
        T: Serializer + Default,
    {
        self.leveldb.get(k)
    }
    //从区块属性加载区块信息
    pub fn load(&mut self, attr: &BlkAttr) -> Result<Arc<Block>, Error> {
        let id = attr.bhv.id()?;
        let key: IKey = id.as_ref().into();
        //如果缓存中存在
        if let Ok(v) = self.cache.get(&key) {
            return Ok(v);
        }
        //读取区块数据
        let buf = self.blk.pull(&attr.blk)?;
        //解析成区块
        let mut reader = Reader::new(&buf);
        let mut blk: Block = reader.decode()?;
        //保存区块其他属性
        blk.hhv = attr.hhv;
        blk.blk = attr.blk.clone();
        blk.rev = attr.rev.clone();
        //加入缓存并返回
        self.cache.put(&key, &blk)
    }
    /// 从索引中获取区块
    /// 获取的区块只能读取
    pub fn get(&mut self, k: &IKey) -> Result<Arc<Block>, Error> {
        //u32 height读取,对应一个hashid
        if k.is_height_key() {
            let ref ik: Hasher = self.leveldb.get(k)?;
            return self.get(&ik.into());
        }
        //如果缓存中存在
        if let Ok(v) = self.cache.get(k) {
            return Ok(v);
        }
        //从数据库查询并加入缓存
        let attr: BlkAttr = self.attr(k)?;
        //读取区块数据
        let buf = self.blk.pull(&attr.blk)?;
        //解析成区块
        let mut reader = Reader::new(&buf);
        let mut blk: Block = reader.decode()?;
        //保存区块其他属性
        blk.hhv = attr.hhv;
        blk.blk = attr.blk;
        blk.rev = attr.rev;
        //加入缓存并返回
        self.cache.put(k, &blk)
    }
    /// 获取交易费用
    pub fn get_tx_transaction_fee(&mut self, tx: &Tx) -> Result<i64, Error> {
        let (mut ofee, mut ifee, mut tfee) = (0, 0, 0);
        //coinbase没有交易费
        if tx.is_coinbase() {
            return Ok(0);
        }
        for inv in tx.ins.iter() {
            //获取引用的输入金额
            let outv = self.get_txin_ref_txout(&inv)?;
            ifee += outv.value;
        }
        for outv in tx.outs.iter() {
            ofee += outv.value;
        }
        if !consts::is_valid_amount(ifee) || !consts::is_valid_amount(ofee) {
            return Error::msg("ifee or ofee error");
        }
        if ofee > ifee {
            return Error::msg("tx out fee > tx input fee");
        }
        //累加交易费
        tfee += ifee - ofee;
        if !consts::is_valid_amount(tfee) {
            return Error::msg("tfee error");
        }
        Ok(tfee)
    }
    /// 获取区块交易费,不检测区块有效性,引用的coin可能已经被消费了,只计算金额
    pub fn get_block_transaction_fee(&mut self, blk: &Block) -> Result<i64, Error> {
        let mut tfee = 0;
        for tx in blk.txs.iter() {
            tfee += self.get_tx_transaction_fee(&tx)?;
        }
        Ok(tfee)
    }
    /// 检测交易签名
    fn check_tx_sign(&mut self, tx: &Tx) -> Result<(), Error> {
        //获取签名器缓存
        let signer = new_tx_signer(self, tx);
        let cache = signer.get_sign_cache(&tx)?;
        for inv in tx.ins.iter() {
            //coinbase交易没有签名
            if inv.is_coinbase() {
                continue;
            }
            //获取引用的输出
            let outv = self.get_txin_ref_txout(&inv)?;
            //验证签名环境
            let env = &LinkExectorEnv {
                tx: &tx,
                inv: &inv,
                outv: &outv,
                cache: &cache,
                singer: Box::new(&*signer),
            };
            //连接输入输出脚本允许检测脚本
            let mut script = inv.script.clone();
            let script = script.concat(&outv.script);
            let mut exector = Exector::new();
            exector.exec(&script, env)?;
        }
        Ok(())
    }
    /// 检测进入交易池的交易
    /// 返回tfee(交易费),cfee(coinbase输出)
    fn check_tx_amount(&mut self, height: u32, tx: &Tx) -> Result<(i64, i64), Error> {
        let (mut ofee, mut ifee, mut tfee, mut cfee) = (0, 0, 0, 0);
        for inv in tx.ins.iter() {
            //coinbase交易不包含金额信息
            if inv.is_coinbase() {
                continue;
            }
            //获取消费地址hasher
            let addr = inv.get_address()?;
            //获取引用的输出
            let outv = self.get_txin_ref_txout(&inv)?;
            //地址是否一致
            if addr != outv.get_address()? {
                return Error::msg("cost addr != out addr");
            }
            //获取引用的金额
            let coin = self.get_coin(&addr, &inv.out, inv.idx)?;
            //地址是否一致
            if addr != coin.get_address()? {
                return Error::msg("cost addr != coin addr");
            }
            //金额是否一致
            if outv.value != coin.value {
                return Error::msg("out value != coin value");
            }
            //金额是否成熟
            if !coin.is_valid(height) {
                return Error::msg("coin not valid");
            }
            //消耗
            ifee += coin.value;
        }
        for outv in tx.outs.iter() {
            if !consts::is_valid_amount(outv.value) {
                return Error::msg("outv value error");
            }
            if outv.value == 0 {
                return Error::msg("outv value error");
            }
            if tx.is_coinbase() {
                cfee += outv.value;
            } else {
                ofee += outv.value;
            }
        }
        if !consts::is_valid_amount(ifee) || !consts::is_valid_amount(ofee) {
            return Error::msg("ifee or ofee error");
        }
        if ofee > ifee {
            return Error::msg("tx out fee > tx input fee");
        }
        //累加交易费
        tfee += ifee - ofee;
        if !consts::is_valid_amount(tfee) {
            return Error::msg("tfee  error");
        }
        if !consts::is_valid_amount(cfee) {
            return Error::msg("cfee  error");
        }
        Ok((tfee, cfee))
    }
    /// 检测区块的金额
    /// 这个检测的区块是新连入的区块,需要检测引用的coin是否在链中
    /// 并且检测签名是否正确
    fn check_block_amount(&mut self, height: u32, blk: &Block) -> Result<(), Error> {
        //coinbase输出，输入, 输出, 交易费, 区块奖励
        //1.每个交易的输出<=输入,差值就是交易费用，可包含在coinbase输出中
        //3.coinbase输出 <= (区块奖励+交易费)
        let (mut cfee, mut tfee, rfee) = (0, 0, self.conf.compute_reward(height));
        if !consts::is_valid_amount(rfee) {
            return Error::msg("rfee  error");
        }
        for tx in blk.txs.iter() {
            //检测交易签名
            self.check_tx_sign(&tx)?;
            //检测交易金额,并返回交易费和coin输出金额(如果是coinbase交易)
            let (tfeev, cfeev) = self.check_tx_amount(height, &tx)?;
            //累加交易费
            tfee += tfeev;
            if !consts::is_valid_amount(tfee) {
                return Error::msg("tfee  error");
            }
            cfee += cfeev;
            if !consts::is_valid_amount(cfee) {
                return Error::msg("cfee  error");
            }
        }
        if !consts::is_valid_amount(rfee) {
            return Error::msg("cfee or rfee error");
        }
        //coinbase输出金额不能 > 奖励 + 交易费
        if cfee > rfee + tfee {
            return Error::msg("cfee > rfee + tfee");
        }
        Ok(())
    }
    /// 链接一个新的区块
    /// 返回顶部区块信息
    /// 写入的数据:
    /// best 顶部区块id和高度
    /// height->block id 高度对应的区块id
    /// block id->block attr 区块id对应的区块信息
    /// blk data 区块数据
    /// rev data 回退数据
    pub fn link(&mut self, blk: &Block) -> Result<Best, Error> {
        //检测基本数据
        blk.check_value(self)?;
        let id = blk.id()?;
        let ref key: IKey = id.as_ref().into();
        if self.leveldb.has(key) {
            return Error::msg("block exists");
        }
        //检测工作难度是否达到设置的要求
        if !id.verify_pow(&self.conf.pow_limit, blk.header.bits) {
            return Error::msg("block bits error");
        }
        //计算并检测下个区块难度,当前链入的区块难度应该和计算出来的一致
        let bits = self.next_bits()?;
        if blk.header.bits != bits {
            return Error::msg("link block bits error");
        }
        //开始写入
        let mut batch = IBatch::new(true);
        //最新best数据
        let mut next = Best {
            id: id.clone(),
            height: u32::MAX,
        };
        //获取顶部区块
        match self.best() {
            Ok(top) => {
                //获取上个区块信息
                let prev = self.get(&top.id_key())?;
                //看当前prev是否指向上个区块
                if blk.header.prev != prev.id()? {
                    return Error::msg("block prev != prev.id");
                }
                next.height = top.next();
                //写入新的并保存旧的到回退数据
                batch.set(&Self::BEST_KEY.into(), &next, &top);
            }
            _ => {
                //第一个区块符合配置的上帝区块就直接写入
                if id != self.conf.genesis {
                    return Error::msg("first block not config genesis");
                }
                next.height = 0;
                batch.put(&Self::BEST_KEY.into(), &next);
            }
        }
        //检测区块的金额和签名
        self.check_block_amount(next.height, blk)?;
        //高度对应的区块id
        batch.put(&next.height_key(), &next.id);
        //每个交易对应的区块信息和位置
        for (i, tx) in blk.txs.iter().enumerate() {
            let txid = &tx.id()?;
            //如果此交易已经存在
            if self.leveldb.has(&txid.into()) {
                return Error::msg("txid exists block index");
            }
            let txattr = TxAttr {
                blk: id.clone(), //此交易指向的区块
                idx: i as u16,   //在区块的位置
            };
            batch.put(&txid.into(), &txattr);
            //写入交易金额
            self.write_tx_index(&mut batch, &next, &tx)?;
        }
        //id对应的区块头属性
        let mut attr = BlkAttr::default();
        //当前区块头
        attr.bhv = blk.header.clone();
        //当前区块高度
        attr.hhv = next.height;
        //获取区块数据,回退数据并写入
        let blkwb = blk.bytes();
        let revwb = batch.reverse();
        //写二进制数据(区块内容和回退数据)
        attr.blk = self.blk.push(blkwb.bytes())?;
        attr.rev = self.rev.push(revwb.bytes())?;
        //写入区块id对应的区块头属性,这个不会包含在回退数据中
        batch.put(key, &attr);
        //批量写入
        self.leveldb.write(&batch, true)?;
        //连接成功将交易池中有的交易移除
        Ok(next)
    }
    /// 获取交易信息
    fn get_tx(&mut self, id: &Hasher) -> Result<Tx, Error> {
        //获取交易对应的存储属性
        let attr: TxAttr = self.attr(&id.as_ref().into())?;
        //获取对应的区块信息
        let blk = self.get(&attr.blk.as_ref().into())?;
        //获取对应的交易信息
        let tx = blk.get_tx(attr.idx as usize)?;
        Ok(tx.clone())
    }
    /// 获取输入引用的输出
    fn get_txin_ref_txout(&mut self, inv: &TxIn) -> Result<TxOut, Error> {
        //获取交易对应的存储属性
        let attr: TxAttr = self.attr(&inv.out.as_ref().into())?;
        //获取对应的区块信息
        let blk = self.get(&attr.blk.as_ref().into())?;
        //获取对应的交易信息
        let tx = blk.get_tx(attr.idx as usize)?;
        //获取对应的输出
        let outv = tx.get_out(inv.idx as usize)?;
        Ok(outv.clone())
    }
    /// 获取输入引用的金额
    pub fn get_txin_ref_coin(&mut self, inv: &TxIn) -> Result<CoinAttr, Error> {
        //获取交易对应的存储属性
        let attr: TxAttr = self.attr(&inv.out.as_ref().into())?;
        //获取区块信息
        let blk = self.get(&attr.blk.as_ref().into())?;
        //获取对应的交易信息
        let tx = blk.get_tx(attr.idx as usize)?;
        //获取对应的输出
        let outv = tx.get_out(inv.idx as usize)?;
        //创建coin信息返回
        let mut coin = CoinAttr::default();
        coin.cpk = outv.get_address()?;
        coin.tx = tx.id()?;
        coin.idx = inv.idx;
        coin.value = outv.value;
        if tx.is_coinbase() {
            coin.flags = COIN_ATTR_FLAGS_COINBASE;
        }
        coin.height = blk.hhv;
        Ok(coin)
    }
    /// 获取金额信息
    /// acc: 账户hash id
    /// tx:交易hash id
    /// idx:输出未知
    pub fn get_coin(&mut self, acc: &Hasher, tx: &Hasher, idx: u16) -> Result<CoinAttr, Error> {
        //如果已经在交易池中
        let mut coin = CoinAttr::default();
        coin.cpk = acc.clone();
        coin.tx = tx.clone();
        coin.idx = idx;
        let mut cv: CoinAttr = self.attr(&coin.key())?;
        cv.cpk = coin.cpk;
        cv.tx = coin.tx;
        cv.idx = coin.idx;
        Ok(cv)
    }
    /// 写入交易索引
    fn write_tx_index(
        &mut self,
        batch: &mut IBatch, //批事务写入
        best: &Best,        //当前生成区块的高度
        tx: &Tx,            //当前区块交易
    ) -> Result<(), Error> {
        let mut flags: u8 = 0;
        if tx.is_coinbase() {
            flags = COIN_ATTR_FLAGS_COINBASE;
        }
        //输入对应消耗金额
        for inv in tx.ins.iter() {
            //coinbase输入不存在金额消耗
            if inv.is_coinbase() {
                continue;
            }
            //获取引用的金额
            let coin = self.get_txin_ref_coin(&inv)?;
            //在当前高度上是否成熟
            if !coin.is_valid(best.height) {
                return Error::msg("ref coin not valid");
            }
            //删除coin并且存入回退数据
            batch.del(&coin.key(), Some(&coin));
        }
        //输出对应获取的金额
        for (i, outv) in tx.outs.iter().enumerate() {
            let mut coin = CoinAttr::default();
            coin.cpk = outv.get_address()?;
            coin.tx = tx.id()?;
            coin.idx = i as u16;
            coin.value = outv.value;
            coin.flags = flags;
            coin.height = best.height;
            batch.put_attr(&coin);
        }
        Ok(())
    }
    /// 回退一个区块,回退多个连续调用此方法
    /// 返回被回退的区块
    pub fn pop(&mut self) -> Result<Block, Error> {
        //获取区块链最高区块属性
        let best = self.best()?;
        if best.id == self.conf.genesis {
            return Error::msg("genesis block can't pop");
        }
        let ref idkey = best.id_key();
        let attr: BlkAttr = self.leveldb.get(idkey)?;
        //读取区块数据
        let buf = self.blk.pull(&attr.blk)?;
        let blk: Block = Reader::unpack(&buf)?;
        //读取回退数据
        let buf = self.rev.pull(&attr.rev)?;
        let mut batch: IBatch = buf[..].try_into()?;
        //删除最后一个区块
        batch.del::<Block>(idkey, None);
        //删除缓存
        self.cache.pop(idkey);
        //批量写入
        self.leveldb.write(&batch, true)?;
        Ok(blk)
    }
}

/// 链线程安全封装
pub struct Chain(RwLock<BlkIndexer>);

impl Chain {
    /// 获取账户池
    pub fn get_account_pool(&self) -> Result<Arc<dyn AccountPool>, Error> {
        self.do_read(|v| v.get_account_pool())
    }
    /// 设置账户池
    pub fn set_account_pool(&self, acp: Arc<dyn AccountPool>) -> Result<(), Error> {
        self.do_write(|v| v.set_account_pool(acp))
    }
    /// 创建交易助手
    pub fn new_helper<'a>(&'a self) -> TxHelper<'a> {
        TxHelper::new(self)
    }
    /// 从当前链顶创建一个新区块
    pub fn new_block<F>(&self, cbstr: &str, ff: F) -> Result<Block, Error>
    where
        F: FnOnce(&mut Block) -> Result<(), Error>,
    {
        self.do_write(|v| v.new_block(cbstr, ff))
    }
    /// lock read process
    fn do_read<R, F>(&self, f: F) -> Result<R, Error>
    where
        F: FnOnce(&BlkIndexer) -> Result<R, Error>,
    {
        self.0.read().map_or_else(Error::std, |ref v| f(v))
    }
    /// lock write process
    fn do_write<R, F>(&self, f: F) -> Result<R, Error>
    where
        F: FnOnce(&mut BlkIndexer) -> Result<R, Error>,
    {
        self.0.write().map_or_else(Error::std, |ref mut v| f(v))
    }
    /// 获取当前配置
    pub fn config(&self) -> Result<Config, Error> {
        self.do_read(|v| Ok(v.config().clone()))
    }
    /// 创建指定路径存储的链
    pub fn new(conf: &Config) -> Result<Self, Error> {
        Ok(Chain(RwLock::new(BlkIndexer::new(conf)?)))
    }
    /// 获取区块链顶部信息
    pub fn best(&self) -> Result<Best, Error> {
        self.do_read(|v| v.best())
    }
    /// 获取属性信息,不缓存
    pub fn attr<T>(&self, k: &IKey) -> Result<T, Error>
    where
        T: Serializer + Default,
    {
        self.do_read(|v| v.attr(k))
    }
    /// 获取账户对应的金额列表
    pub fn coins(&self, acc: &Account) -> Result<Vec<CoinAttr>, Error> {
        self.do_read(|v| v.coins(acc))
    }
    /// 获取交易信息
    pub fn get_tx(&self, k: &IKey) -> Result<Tx, Error> {
        let attr: TxAttr = self.attr(k)?;
        let blk = self.get(&attr.blk.as_ref().into())?;
        let tx = blk.get_tx(attr.idx as usize)?;
        Ok(tx.clone())
    }
    /// 获取输入引用的输出
    pub fn get_txin_ref_txout(&self, inv: &TxIn) -> Result<TxOut, Error> {
        //获取对应的交易信息
        let tx = self.get_tx(&inv.out.as_ref().into())?;
        //获取对应的输出
        let outv = tx.get_out(inv.idx as usize)?;
        //复制返回
        Ok(outv.clone())
    }
    /// 从属性加载区块信息
    pub fn load(&self, attr: &BlkAttr) -> Result<Arc<Block>, Error> {
        self.do_write(|v| v.load(attr))
    }
    /// 获取区块信息
    /// k可以是高度或者id
    pub fn get(&self, k: &IKey) -> Result<Arc<Block>, Error> {
        self.do_write(|v| v.get(k))
    }
    /// 链接一个新区块到链上
    pub fn link(&self, blk: &Block) -> Result<Best, Error> {
        self.do_write(|ctx| ctx.link(blk))
    }
    /// 弹出一个区块
    pub fn pop(&self) -> Result<Block, Error> {
        self.do_write(|v| v.pop())
    }
    /// 添加交易到交易池
    pub fn append(&self, tx: &Tx) -> Result<Hasher, Error> {
        self.do_write(|v| v.append(tx))
    }
    /// 从交易池移除交易
    pub fn remove(&self, id: &Hasher) -> Result<Arc<Tx>, Error> {
        self.do_write(|v| v.remove(id))
    }
}

#[test]
fn test_indexer_thread() {
    use std::sync::Arc;
    use std::{thread, time};
    Config::test(|_, idx| {
        let indexer = Arc::new(idx);
        for _ in 0..10 {
            let idx = indexer.clone();
            thread::spawn(move || {
                let b1 = idx.new_block("", |_| Ok(())).unwrap();
                idx.link(&b1).unwrap();
                let id = b1.id().unwrap();
                let b2 = idx.get(&id.as_ref().into()).unwrap();
                assert_eq!(b1, *b2);
            });
        }
        thread::sleep(time::Duration::from_secs(1));
        Ok(())
    })
}

#[test]
fn test_simple_link_pop() {
    Config::test(|conf, idx| {
        let best = idx.best()?;
        assert_eq!(0, best.height);
        for _ in 0u32..=10 {
            let b1 = idx.new_block("", |_| Ok(()))?;
            idx.link(&b1)?;
        }
        let best = idx.best()?;
        assert_eq!(11, best.height);
        for i in 0u32..=10 {
            let best = idx.best()?;
            assert_eq!(11 - i, best.height);
            idx.pop()?;
        }
        let best = idx.best()?;
        assert_eq!(0, best.height);
        assert_eq!(best.id, conf.genesis);
        Ok(())
    });
}

/// 线程安全的区块LRU缓存实现
pub struct BlkCache {
    lru: LruCache<IKey, Arc<Block>>,
}

impl Default for BlkCache {
    fn default() -> Self {
        BlkCache::new(1024 * 10)
    }
}

impl BlkCache {
    /// 创建指定大小的lur缓存
    pub fn new(cap: usize) -> Self {
        BlkCache {
            lru: LruCache::new(cap),
        }
    }
    /// 获取缓存长度
    pub fn len(&self) -> usize {
        self.lru.len()
    }
    /// 加入缓存值
    /// 如果存在将返回旧值
    pub fn put(&mut self, k: &IKey, v: &Block) -> Result<Arc<Block>, Error> {
        let ret = Arc::new(v.clone());
        self.lru.put(k.clone(), ret.clone());
        Ok(ret.clone())
    }
    /// 从缓存获取值,不改变缓存状态
    pub fn peek<F>(&self, k: &IKey) -> Option<Arc<Block>> {
        self.lru.peek(k).map(|v| v.clone())
    }

    /// 检测指定的key是否存在
    pub fn contains(&self, k: &IKey) -> bool {
        self.lru.contains(k)
    }

    /// 从缓存移除数据
    pub fn pop(&mut self, k: &IKey) -> Option<Arc<Block>> {
        self.lru.pop(k)
    }
    /// 从缓存获取值并复制返回
    /// 复制对应的值返回
    pub fn get(&mut self, k: &IKey) -> Result<Arc<Block>, Error> {
        self.lru
            .get(k)
            .map(|v| v.clone())
            .ok_or(Error::error("not found"))
    }
}

#[test]
fn test_lru_write() {
    let c = &mut BlkCache::default();
    let blk = Block::default();
    c.put(&1u32.into(), &blk).unwrap();
    let v = c.get(&1u32.into()).unwrap();
    let ver = v.header.get_ver();
    assert_eq!(ver, 1);
}

#[test]
fn test_lru_cache() {
    let c = &mut BlkCache::default();
    let blk = Block::default();
    c.put(&1u32.into(), &blk).unwrap();
    let v = c.get(&1u32.into()).unwrap();
    assert_eq!(v.as_ref(), &Block::default());
    assert_eq!(1, c.len());
    let v = c.pop(&1u32.into()).unwrap();
    assert_eq!(v.as_ref(), &Block::default());
    assert_eq!(0, c.len());
}

#[test]
fn test_tx_helper() {
    use crate::config::Config;
    use crate::consts;
    Config::test(|conf, idx| {
        //这个账户有钱
        let acc = conf.acc.as_ref().unwrap();
        //创建100个区块
        for _ in 0..consts::COINBASE_MATURITY {
            let b = idx.new_block("", |_| Ok(()))?;
            idx.link(&b)?;
        }
        let best = idx.best()?;
        assert_eq!(best.height, 100);
        let mut helper = idx.new_helper();
        //设置全签名器
        helper.set_signer(FullSigner {})?;
        //获取可用金额
        let coins = idx.coins(&acc)?;
        for coin in coins.iter() {
            if !coin.is_valid(best.next()) {
                continue;
            }
            //添加可用金额记录
            helper.add_coin(coin)?;
        }
        //height: 0 1 区块可用
        assert_eq!(helper.coins.len(), 2);
        //获取账户池
        let accpool = idx.get_account_pool()?;
        //账户1转10*COIN
        let acc1 = accpool.value(0)?;
        helper.add_out(&acc1.string()?, 10 * consts::COIN)?;
        //账户2转20*COIN
        let acc2 = accpool.value(1)?;
        helper.add_out(&acc2.string()?, 20 * consts::COIN)?;
        //交易费
        helper.set_cost_fee(1 * consts::COIN)?;

        let tx = Tx::try_from(&helper)?;
        //新交易放入交易池
        let id = idx.append(&tx)?;
        //从交易池拉取交易列表
        println!("{:x?}", tx);
        Ok(())
    });
}
