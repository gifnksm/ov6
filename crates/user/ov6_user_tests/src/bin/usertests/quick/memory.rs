use core::{ptr, slice};

use ov6_user_lib::{
    error::Ov6Error,
    fs::{self, File},
    io::{Read as _, STDOUT_FD, Write as _},
    os::ov6::syscall,
    pipe, process,
};

use crate::{README_PATH, expect};

const FILE_PATH: &str = "copyin1";

/// what if you pass ridiculous pointers to system calls
/// that read user memory with copyin?
pub fn copy_u2k() {
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

/// what if you pass ridiculous pointers to system calls
/// that write user memory with copyout?
pub fn copy_k2u() {
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

/// See if the kernel refuses to read/write user memory that the
/// application doesn't have anymore, because it returned it.
pub fn rw_sbrk() {
    let a = process::grow_break(8192).unwrap();
    let _ = unsafe { process::shrink_break(8192) }.unwrap();

    let mut file = File::create(FILE_PATH).unwrap();
    unsafe {
        expect!(
            file.write(slice::from_raw_parts(a.add(4096), 1024)),
            Err(Ov6Error::BadAddress),
        );
    }
    drop(file);
    fs::remove_file(FILE_PATH).unwrap();

    let mut file = File::open(README_PATH).unwrap();
    unsafe {
        expect!(
            file.read(slice::from_raw_parts_mut(a.add(4096), 10)),
            Err(Ov6Error::BadAddress),
        );
    }
    drop(file);
}
