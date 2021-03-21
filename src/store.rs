use crate::errors::Error;
use crate::iobuf::{Reader, Serializer, Writer};
use crate::util;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// 区块存储属性
#[derive(Debug, PartialEq)]
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
    /// 存储文件路径
    fn store_file_path(dir: &Path, idx: u32, ext: &str) -> PathBuf {
        dir.join(format!("{:08}.{}", idx, ext))
    }
    /// 创建存储文件
    fn new(idx: u32, fs: fs::File) -> Self {
        StoreFile { idx: idx, file: fs }
    }
    /// 获取初始文件
    /// 从当前目录搜索最后一个文件，如果文件大于最大
    /// 创建下一个文件返回
    /// path:路径
    /// ext:文件扩展
    /// max_file_size:单个文件最大存储字节
    fn init_file(path: &str, ext: &str, max_file_size: u32) -> Result<Self, Error> {
        let dir = Path::new(path);
        let reader = fs::read_dir(dir);
        if let Err(err) = reader {
            return Error::std(err);
        }
        let reader = &mut reader.unwrap();
        let mut max = 0u32;
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
        let last = Self::store_file_path(dir, max, ext);
        if fs::metadata(&last).map_or(0, |v| v.len() as u32) > max_file_size {
            max += 1;
        }
        //打开下个文件
        let last = Self::store_file_path(dir, max, ext);
        fs::OpenOptions::new()
            .append(true)
            .read(true)
            .create(true)
            .open(&last)
            .map_or_else(Error::std, |v| Ok(StoreFile::new(max, v)))
    }
    fn metadata(&self) -> Result<fs::Metadata, Error> {
        self.file.metadata().map_or_else(Error::std, |v| Ok(v))
    }
    //打开只读的文件存储
    fn open_only_read(idx: u32, path: &str, ext: &str) -> Result<Self, Error> {
        let dir = Path::new(path);
        let last = Self::store_file_path(dir, idx, ext);
        fs::OpenOptions::new()
            .read(true)
            .open(&last)
            .map_or_else(Error::std, |v| Ok(StoreFile::new(idx, v)))
    }
    /// 同步数据到磁盘
    fn sync(&self) -> Result<(), Error> {
        self.file.sync_data().map_or_else(Error::std, |_| Ok(()))
    }
    /// 获取当前文件指针
    fn pos(&self) -> Result<u64, Error> {
        let file = &mut &self.file;
        file.seek(SeekFrom::Current(0))
            .map_or_else(Error::std, |v| Ok(v))
    }
    /// 移动文件指针到指定位置
    fn seek(&self, pos: u64) -> Result<u64, Error> {
        let file = &mut &self.file;
        file.seek(SeekFrom::Start(pos))
            .map_or_else(Error::std, |v| Ok(v))
    }
    /// 追加写入所有数据,seek不会改变写入的位置,只会写入文件末尾
    /// 写完后读写指针移动到危机末尾
    /// 返回实际写入的长度
    fn append(&self, b: &[u8]) -> Result<usize, Error> {
        let file = &mut &self.file;
        let mut p = 0;
        let l = b.len();
        while l - p > 0 {
            let wl = file.write(&b[p..l]).map_or_else(Error::std, |v| Ok(v))?;
            if wl <= 0 {
                return Error::msg("write error");
            }
            p += wl;
        }
        Ok(p)
    }
    /// 读取所有数据
    /// 返回实际读取的长度
    fn read(&self, buf: &mut [u8]) -> Result<usize, Error> {
        let file = &mut &self.file;
        let mut p = 0;
        let l = buf.len();
        while l - p > 0 {
            let rl = file
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
    fs.append("12345678".as_bytes()).unwrap();
    assert_eq!(8, fs.pos().unwrap());
    fs.seek(0).unwrap();
    assert_eq!(0, fs.pos().unwrap());
    fs.append("87654321".as_bytes()).unwrap();
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
    idx: u32,
    max: u32,
    dir: String, //存储目录
    ext: String, //文件后缀
    cache: Vec<StoreFile>,
}

impl Store {
    const MAX_CACHE_FILE: usize = 16;
    ///  移除idx最小的那个
    /// 不移除当前写入文件
    fn remove_file(&mut self) {
        let mut rmin = u32::MAX;
        let mut ridx = usize::MAX;
        for i in 0..self.cache.len() {
            let file = &self.cache[i];
            //不移除ref前写入文件
            if file.idx == self.idx {
                continue;
            }
            //获取最小的文件索引
            if file.idx < rmin {
                rmin = file.idx;
                ridx = i;
            }
        }
        if ridx != usize::MAX {
            self.cache.remove(ridx);
        }
    }
    /// 打开新文件
    fn open_file(&mut self, idx: u32) -> Result<&StoreFile, Error> {
        let fs = StoreFile::open_only_read(idx, &self.dir, &self.dir)?;
        self.cache.push(fs);
        if self.cache.len() > Self::MAX_CACHE_FILE {
            self.remove_file();
        }
        Ok(self.cache.last().unwrap())
    }
    /// 获取缓存文件
    fn cache_file(&self, idx: u32) -> Option<&StoreFile> {
        self.cache.iter().filter(|v| v.idx == idx).next()
    }
    /// 获取当前最大的写入文件
    fn curr_file(&self) -> Result<&StoreFile, Error> {
        self.cache_file(self.idx)
            .map_or(Error::msg("not found max file"), |v| Ok(v))
    }
    /// 检测是否切换到下个文件
    /// 返回写入前的位置
    fn check_next(&mut self, l: u32) -> Result<u32, Error> {
        if l > self.max {
            return Error::msg("max must > push bytes len");
        }
        let dir = Path::new(&self.dir);
        //获取当前写入文件
        let sf = self.curr_file()?;
        //检测文件大小,能写入就返回当前文件位置
        let pos = sf.metadata()?.len() as u32;
        if (pos + l) <= self.max as u32 {
            return Ok(pos);
        }
        //创建下一个文件
        let next = StoreFile::store_file_path(dir, self.idx + 1, &self.ext);
        fs::OpenOptions::new()
            .append(true)
            .read(true)
            .create(true)
            .open(&next)
            .map_or_else(Error::std, |file| {
                self.idx += 1;
                let sf = StoreFile::new(self.idx, file);
                self.cache.push(sf);
                Ok(0) //新创建的从0开始
            })
    }
    /// 创建区块存储器
    pub fn new(dir: &str, ext: &str, max: u32) -> Result<Self, Error> {
        let dir: String = dir.into();
        util::miss_create_dir(&dir)?;
        let sf = StoreFile::init_file(&dir, ext, max)?;
        Ok(Store {
            idx: sf.idx,
            max: max,
            dir: dir,
            ext: ext.into(),
            cache: vec![sf],
        })
    }
    /// 追加写入数据
    /// 返回写入前的文件长度
    pub fn push(&mut self, b: &[u8]) -> Result<Attr, Error> {
        let bl = b.len() as u32;
        if bl == 0 {
            return Error::msg("push empty data");
        }
        let pos = self.check_next(bl)?;
        let sf = self.curr_file()?;
        sf.append(b)?;
        Ok(Attr {
            idx: self.idx,
            off: pos,
            size: bl,
        })
    }
    /// 读取buf指定大小的数据
    pub fn read(&mut self, i: u32, p: u32, buf: &mut [u8]) -> Result<(), Error> {
        let sf: &StoreFile;
        if let Some(v) = self.cache_file(i) {
            sf = v;
        } else if let Ok(v) = self.open_file(i) {
            sf = v;
        } else {
            return Error::msg("open file error");
        }
        sf.seek(p as u64)?;
        sf.read(buf)?;
        Ok(())
    }
    /// 读取数据
    /// i文件, p读取位置, l读取长度
    pub fn pull(&mut self, i: u32, p: u32, l: u32) -> Result<Vec<u8>, Error> {
        let sf: &StoreFile;
        if let Some(v) = self.cache_file(i) {
            sf = v;
        } else if let Ok(v) = self.open_file(i) {
            sf = v;
        } else {
            return Error::msg("open file error");
        }
        let mut buf = vec![0u8; l as usize];
        sf.seek(p as u64)?;
        sf.read(&mut buf)?;
        Ok(buf)
    }
}
#[test]
fn test_store() {
    use tempdir::TempDir;
    let tmp = TempDir::new("store").unwrap();
    let dir = tmp.path().to_str().unwrap();
    let mut store = Store::new(dir, "blk", 30).unwrap();
    let attr = store
        .push(&[1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12])
        .unwrap();
    assert_eq!(
        attr,
        Attr {
            idx: 0,
            off: 0,
            size: 12
        }
    );
    let attr = store.push(&[1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10]).unwrap();
    assert_eq!(
        attr,
        Attr {
            idx: 0,
            off: 12,
            size: 10
        }
    );
    let buf = store.pull(0, 0, 12).unwrap();
    assert_eq!(&buf, &[1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
    assert_eq!(1, store.cache.len());
    assert_eq!(0, store.idx);
}
