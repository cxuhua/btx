use crate::errors::Error;
use crate::iobuf::{Reader, Serializer, Writer};
use crate::util;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::RwLock;

/// 单个文件最大存放的数据长度
const FILE_MAX_SIZE: u64 = 1024 * 1024 * 512;

/// 区块存储属性
pub struct Attr {
    pub idx: u32,  //所在文件
    pub off: u32,  //文件偏移
    pub size: u32, //文件大小
}

impl Default for Attr {
    fn default() -> Self {
        Attr {
            idx: 0,
            off: 0,
            size: 0,
        }
    }
}

impl Serializer for Attr {
    fn encode(&self, w: &mut Writer) {
        w.u32(self.idx);
        w.u32(self.off);
        w.u32(self.size);
    }
    fn decode(r: &mut Reader) -> Result<Self, Error>
    where
        Self: Default,
    {
        let mut attr = Attr::default();
        attr.idx = r.u32()?;
        attr.off = r.u32()?;
        attr.size = r.u32()?;
        Ok(attr)
    }
}

/// 文件头
pub struct Header {
    pub ver: u32, //版本
}

impl Default for Header {
    fn default() -> Self {
        Header { ver: 1 }
    }
}

impl Serializer for Header {
    fn encode(&self, w: &mut Writer) {
        w.u32(self.ver);
    }
    fn decode(r: &mut Reader) -> Result<Self, Error>
    where
        Self: Default,
    {
        let mut v = Header::default();
        v.ver = r.u32()?;
        Ok(v)
    }
}

/// 存储文件
#[derive(Debug)]
struct StoreFile {
    idx: u32,
    file: fs::File,
}

impl StoreFile {
    /// 获取初始文件
    /// 从当前目录搜索最后一个文件，如果文件大于最大
    /// 创建下一个文件返回
    fn init_file(path: &str, ext: &str) -> Result<Self, Error> {
        let dir = Path::new(path);
        let dir = fs::read_dir(dir);
        if let Err(err) = dir {
            return Error::std(err);
        }
        let dir = &mut dir.unwrap();
        let mut max = 0;
        loop {
            let item = dir.next();
            if item.is_none() {
                break;
            }
            let item = item.unwrap();
            if item.is_err() {
                continue;
            }
            let item = item.unwrap();
            //是否是文件
            if !item.file_type().map_or(false, |v| v.is_file()) {
                continue;
            }
            let path = item.path();
            //扩展名是否正确
            if !path.extension().map_or(false, |v| v == ext) {
                continue;
            }
            let stem: String = path.file_stem().map_or("", |v| v.to_str().unwrap()).into();
            if let Ok(idx) = stem.parse::<u32>() {
                if idx > max {
                    max = idx
                }
            }
        }
        let last = format!("{}/{:08}.{}", path, max, ext);
        if fs::metadata(Path::new(&last)).map_or(0, |v| v.len()) > FILE_MAX_SIZE {
            max += 1;
        }
        let last = format!("{}/{:08}.{}", path, max, ext);
        let mut opts = fs::OpenOptions::new();
        opts.append(true)
            .read(true)
            .create(true)
            .write(true)
            .open(Path::new(&last))
            .map_or_else(Error::std, |v| Ok(StoreFile { idx: max, file: v }))
    }
}

#[test]
fn test_store_file() {
    use tempdir::TempDir;
    let tmp = TempDir::new("store").unwrap();
    for i in 0..32 {
        let n = format!("{}/{:08}.log", tmp.path().to_str().unwrap(), i);
        fs::File::create(Path::new(&n)).unwrap();
    }
    let fs = StoreFile::init_file(tmp.path().to_str().unwrap(), "log");
    println!("{:?}", fs);
}

/// 区块数据和回退数据存储
/// .blk 存储区块内容 .rev 存储回退数据
pub struct Store {
    dir: String,             //存储目录
    ext: String,             //文件后缀
    file: RwLock<StoreFile>, //当前写入文件
}

impl Store {
    //获取最后一个文件
    fn last_file() -> Result<StoreFile, Error> {
        Error::msg("not imp")
    }
    /// 创建区块存储器
    pub fn new(dir: &str, ext: &str) -> Result<Self, Error> {
        let dir: String = dir.into();
        util::miss_create_dir(&dir)?;
        Ok(Store {
            dir: dir,
            ext: ext.into(),
            file: RwLock::new(Self::last_file()?),
        })
    }
}
#[test]
fn test_store() {
    let store = Store::new("./data/block", "blk");
}
