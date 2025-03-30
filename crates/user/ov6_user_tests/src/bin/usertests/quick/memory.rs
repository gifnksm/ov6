use core::slice;

use ov6_syscall::{UserMutSlice, UserSlice, error::SyscallError, syscall};
use ov6_user_lib::{
    error::Ov6Error,
    fs::{self, File},
    io::{Read as _, STDOUT_FD, Write as _},
    os::{fd::AsRawFd as _, ov6::syscall::ffi::SyscallExt as _},
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
        let file = File::create(FILE_PATH).unwrap();
        expect!(
            syscall::Write::call((file.as_raw_fd(), UserSlice::from_raw_parts(addr, 8192))),
            Err(SyscallError::BadAddress),
            "addr={addr:?}",
        );
        drop(file);
        fs::remove_file(FILE_PATH).unwrap();

        expect!(
            syscall::Write::call((STDOUT_FD, UserSlice::from_raw_parts(addr, 8192))),
            Err(SyscallError::BadAddress),
            "addr={addr:?}",
        );

        let (_rx, tx) = pipe::pipe().unwrap();
        expect!(
            syscall::Write::call((tx.as_raw_fd(), UserSlice::from_raw_parts(addr, 8192))),
            Err(SyscallError::BadAddress),
            "addr={addr:?}",
        );
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
        let file = File::open(README_PATH).unwrap();

        expect!(
            syscall::Read::call((file.as_raw_fd(), UserMutSlice::from_raw_parts(addr, 8192))),
            Err(SyscallError::BadAddress),
            "addr={addr:?}"
        );
        drop(file);

        let (rx, mut tx) = pipe::pipe().unwrap();
        tx.write_all(b"x").unwrap();
        expect!(
            syscall::Read::call((rx.as_raw_fd(), UserMutSlice::from_raw_parts(addr, 8192))),
            Err(SyscallError::BadAddress),
            "addr={addr:?}"
        );
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
