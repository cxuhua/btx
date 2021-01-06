use std::mem;
use std::ptr;
use std::slice;

pub fn encode<T: Sized>(src: &T) -> &[u8] {
    unsafe { slice::from_raw_parts((src as *const T) as *const u8, mem::size_of::<T>()) }
}

pub fn decode<T: Sized>(src: &[u8]) -> T {
    unsafe {
        assert_eq!(src.len(), mem::size_of::<T>());
        ptr::read(src.as_ptr() as *const T)
    }
}

#[test]
fn test_u32() {
    let ss: u32 = 12;
    let s1 = encode(&ss);
    let v: u32 = decode(s1);
    assert_eq!(ss, v)
}

