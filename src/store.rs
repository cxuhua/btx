use crate::errors::Error;
use crate::iobuf::{Reader, Serializer, Writer};
use crate::util;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::RwLock;

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
    /// path:路径
    /// ext:文件扩展
    /// max_file_size:单个文件最大存储字节
    fn init_file(path: &str, ext: &str, max_file_size: u64) -> Result<Self, Error> {
        let dir = Path::new(path);
        let reader = fs::read_dir(dir);
        if let Err(err) = reader {
            return Error::std(err);
        }
        let reader = &mut reader.unwrap();
        let mut max = 0;
        for item in reader {
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
        let last = dir.join(format!("{:08}.{}", max, ext));
        if fs::metadata(&last).map_or(0, |v| v.len()) > max_file_size {
            max += 1;
        }
        let last = dir.join(format!("{:08}.{}", max, ext));
        let mut opts = fs::OpenOptions::new();
        opts.append(true)
            .read(true)
            .create(true)
            .open(&last)
            .map_or_else(Error::std, |v| Ok(StoreFile { idx: max, file: v }))
    }
    /// 检测是否切换到下个文件-
    /// 返回下个文件的编码
    /// path:路径
    /// ext:文件扩展
    /// max_file_size:单个文件最大存储字节
    fn check_next(&mut self, path: &str, ext: &str, max_file_size: u64) -> Result<u32, Error> {
        let dir = Path::new(path);
        let meta = self.file.metadata().map_or_else(Error::std, |v| Ok(v))?;
        if meta.len() <= max_file_size {
            return Ok(self.idx);
        }
        self.idx += 1;
        let last = dir.join(format!("{:08}.{}", self.idx, ext));
        let mut opts = fs::OpenOptions::new();
        opts.append(true)
            .read(true)
            .create(true)
            .open(&last)
            .map_or_else(Error::std, |v| {
                self.file = v;
                Ok(self.idx)
            })
    }
    /// 同步数据到磁盘
    fn sync(&mut self) -> Result<(), Error> {
        self.file.sync_data().map_or_else(Error::std, |_| Ok(()))
    }
    /// 获取当前文件指针
    fn pos(&mut self) -> Result<u64, Error> {
        self.file
            .seek(SeekFrom::Current(0))
            .map_or_else(Error::std, |v| Ok(v))
    }
    /// 移动文件指针到指定位置
    fn seek(&mut self, pos: u64) -> Result<u64, Error> {
        self.file
            .seek(SeekFrom::Start(pos))
            .map_or_else(Error::std, |v| Ok(v))
    }
    /// 追加写入所有数据,seek不会改变写入的位置,只会写入文件末尾
    /// 写完后读写指针移动到危机末尾
    /// 返回实际写入的长度
    fn write(&mut self, b: &[u8]) -> Result<usize, Error> {
        let mut p = 0;
        let l = b.len();
        while l - p > 0 {
            let wl = self
                .file
                .write(&b[p..l])
                .map_or_else(Error::std, |v| Ok(v))?;
            if wl <= 0 {
                return Error::msg("write error");
            }
            p += wl;
        }
        Ok(p)
    }
    /// 读取所有数据
    /// 返回实际读取的长度
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        let mut p = 0;
        let l = buf.len();
        while l - p > 0 {
            let rl = self
                .file
                .read(&mut buf[p..l])
                .map_or_else(Error::std, |v| Ok(v))?;
            if rl <= 0 {
                return Error::msg("read error");
            }
            p += rl;
        }
        Ok(p)
    }
}

#[test]
fn test_check_next_file() {
    use tempdir::TempDir;
    let tmp = TempDir::new("store").unwrap();
    let dir = tmp.path().to_str().unwrap();
    for i in 0..1 {
        let n = format!("{}/{:08}.log", dir, i);
        fs::File::create(Path::new(&n)).unwrap();
    }
    let fs = &mut StoreFile::init_file(dir, "log", 3).unwrap();
    assert_eq!(0, fs.idx);
    fs.write("1234".as_bytes()).unwrap();
    fs.sync().unwrap();
    fs.check_next(dir, "log", 3).unwrap();
    assert_eq!(1, fs.idx);

    fs.write("1234".as_bytes()).unwrap();
    fs.sync().unwrap();
    fs.check_next(dir, "log", 3).unwrap();
    assert_eq!(2, fs.idx);

    fs.write("1234".as_bytes()).unwrap();
    fs.sync().unwrap();
    fs.check_next(dir, "log", 3).unwrap();
    assert_eq!(3, fs.idx);
}

#[test]
fn test_store_file() {
    use tempdir::TempDir;
    let tmp = TempDir::new("store").unwrap();
    let dir = tmp.path().to_str().unwrap();
    for i in 0..32 {
        let n = format!("{}/{:08}.log", dir, i);
        fs::File::create(Path::new(&n)).unwrap();
    }
    let fs = &mut StoreFile::init_file(dir, "log", 1024).unwrap();
    assert_eq!(31, fs.idx);
    assert_eq!(true, fs.file.metadata().unwrap().is_file());
    fs.write("12345678".as_bytes()).unwrap();
    assert_eq!(8, fs.pos().unwrap());
    fs.seek(0).unwrap();
    assert_eq!(0, fs.pos().unwrap());
    fs.write("87654321".as_bytes()).unwrap();
    assert_eq!(16, fs.pos().unwrap());
    fs.seek(0).unwrap();
    let mut buf = [0u8; 16];
    let len = fs.read(&mut buf[..]).unwrap();
    assert_eq!(&buf, b"1234567887654321");
    assert_eq!(16, len);
}

/// 区块数据和回退数据存储
/// .blk 存储区块内容 .rev 存储回退数据
pub struct Store {
    dir: String,             //存储目录
    ext: String,             //文件后缀
    file: RwLock<StoreFile>, //当前写入文件
}

impl Store {
    const FILE_MAX_SIZE: u64 = 1024 * 1024 * 512;
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
