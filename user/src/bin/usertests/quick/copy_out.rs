use core::{ptr, slice};

use ov6_user_lib::{
    error::Ov6Error,
    fs::File,
    io::{Read as _, Write as _},
    pipe,
};

use crate::{README_PATH, expect};

/// what if you pass ridiculous pointers to system calls
/// that write user memory with copyout?
pub fn test() {
    let addrs: &[usize] = &[
        0,
        0x8000_0000,
        0x3f_ffff_e000,
        0x3f_ffff_f000,
        0x40_0000_0000,
        0xffff_ffff_ffff_ffff,
    ];

    for &addr in addrs {
        let addr = ptr::with_exposed_provenance_mut(addr);
        let buf = unsafe { slice::from_raw_parts_mut(addr, 8192) };

        let mut file = File::open(README_PATH).unwrap();

        expect!(file.read(buf), Err(Ov6Error::BadAddress), "addr={addr:p}");
        drop(file);

        let (mut rx, mut tx) = pipe::pipe().unwrap();
        tx.write_all(b"x").unwrap();
        expect!(rx.read(buf), Err(Ov6Error::BadAddress), "addr={addr:p}");
    }
}
