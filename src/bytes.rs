///二机制生成
pub trait Bytes: Sized {
    fn bytes(&self) -> Vec<u8>;
}

//从进制生成
pub trait WithBytes<T>: Bytes {
    fn with_bytes(bb: &Vec<u8>) -> T;
}
