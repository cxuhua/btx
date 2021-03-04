use crate::errors::Error;

///二机制生成
pub trait IntoBytes: Sized {
    fn into_bytes(&self) -> Vec<u8>;
}

//从进制生成
pub trait FromBytes: IntoBytes {
    fn from_bytes(bb: &Vec<u8>) -> Result<Self, Error>;
}
