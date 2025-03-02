use core::{ffi::CStr, ptr, slice};

use xv6_user_lib::{
    error::Error,
    fs::{self, File},
    io::{STDOUT_FD, Write as _},
    os::xv6::syscall,
    pipe,
};

use crate::expect;

const FILE_PATH: &CStr = c"copyin1";

/// what if you pass ridiculous pointers to system calls
/// that read user memory with copyin?
pub fn test() {
    let addrs: &[usize] = &[
        0x80000000,
        0x3fffffe000,
        0x3ffffff000,
        0x4000000000,
        0xffffffffffffffff,
    ];

    for &addr in addrs {
        let addr = ptr::with_exposed_provenance(addr);
        let buf = unsafe { slice::from_raw_parts(addr, 8192) };

        let mut file = File::create(FILE_PATH).unwrap();
        expect!(file.write(buf), Err(Error::Unknown), "addr={addr:p}");
        drop(file);
        fs::remove_file(FILE_PATH).unwrap();

        // FIXME: this should return an error, but it doesn't
        expect!(syscall::write(STDOUT_FD, buf), Ok(0), "addr={addr:p}");

        // FIXME: this should return an error, but it doesn't
        expect!(syscall::write(STDOUT_FD, buf), Ok(0), "addr={addr:p}");

        let (_rx, mut tx) = pipe::pipe().unwrap();
        // FIXME: this should return an error, but it doesn't
        expect!(tx.write(buf), Ok(0), "addr={addr:p}");
    }
}
