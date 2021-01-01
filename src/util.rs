use std::mem;
use std::ptr;
use std::slice;

///将结构数据转换为二进制数据
///
pub unsafe fn binary_encode<T: Sized>(src: &T) -> &[u8] {
    slice::from_raw_parts((src as *const T) as *const u8, mem::size_of::<T>())
}

///将二进制数据转换为结构数据
pub unsafe fn binary_decode<T: Sized>(src: &[u8]) -> T {
    ptr::read(src.as_ptr() as *const T)
}

#[test]
fn test_binary_encode_decode() {
    unsafe {
        let s = 0x11223344;
        let ss = binary_encode(&s);
        let m: u32 = binary_decode(ss);
        assert_eq!(m, s);
    }
}