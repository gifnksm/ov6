use core::{ffi::CStr, ptr, slice};

use ov6_user_lib::{
    error::Ov6Error,
    fs::{self, File},
    io::{STDOUT_FD, Write as _},
    os::ov6::syscall,
    pipe,
};

use crate::expect;

const FILE_PATH: &CStr = c"copyin1";

/// what if you pass ridiculous pointers to system calls
/// that read user memory with copyin?
pub fn test() {
    let addrs: &[usize] = &[
        0x8000_0000,
        0x3f_ffff_e000,
        0x3f_ffff_f000,
        0x40_0000_0000,
        0xffff_ffff_ffff_ffff,
    ];

    for &addr in addrs {
        let addr = ptr::with_exposed_provenance(addr);
        let buf = unsafe { slice::from_raw_parts(addr, 8192) };

        let mut file = File::create(FILE_PATH).unwrap();
        expect!(file.write(buf), Err(Ov6Error::BadAddress), "addr={addr:p}");
        drop(file);
        fs::remove_file(FILE_PATH).unwrap();

        expect!(
            syscall::write(STDOUT_FD, buf),
            Err(Ov6Error::BadAddress),
            "addr={addr:p}"
        );

        let (_rx, mut tx) = pipe::pipe().unwrap();
        expect!(tx.write(buf), Err(Ov6Error::BadAddress), "addr={addr:p}");
    }
}
