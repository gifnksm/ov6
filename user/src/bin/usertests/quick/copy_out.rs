use core::{ptr, slice};

use xv6_user_lib::{
    error::Error,
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
        0x80000000,
        0x3fffffe000,
        0x3ffffff000,
        0x4000000000,
        0xffffffffffffffff,
    ];

    for &addr in addrs {
        let addr = ptr::with_exposed_provenance_mut(addr);
        let buf = unsafe { slice::from_raw_parts_mut(addr, 8192) };

        let mut file = File::open(README_PATH).unwrap();

        // FIXME: this should return an error, but it doesn't
        expect!(file.read(buf), Err(Error::Unknown), "addr={addr:p}");
        drop(file);

        let (mut rx, mut tx) = pipe::pipe().unwrap();
        tx.write_all(b"x").unwrap();
        // FIXME: this should return an error, but it doesn't
        expect!(rx.read(buf), Ok(0), "addr={addr:p}");
    }
}
