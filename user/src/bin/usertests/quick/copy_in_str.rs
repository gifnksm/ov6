use core::{
    ffi::{CStr, c_char},
    ptr,
};

use xv6_kernel_params::MAX_PATH;
use xv6_user_lib::{
    error::Error,
    fs::{self, File},
    process,
};

use crate::{ECHO_PATH, PAGE_SIZE, expect};

/// what if you pass ridiculous string pointers to system calls?
pub fn test1() {
    let addrs: &[usize] = &[
        0x80000000,
        0x3fffffe000,
        0x3ffffff000,
        0x4000000000,
        0xffffffffffffffff,
    ];

    for &addr in addrs {
        let addr = ptr::with_exposed_provenance::<u8>(addr);
        let path = unsafe { &*(ptr::slice_from_raw_parts(addr, 8192) as *const CStr) };

        expect!(File::create(path), Err(Error::Unknown), "addr={addr:p}");
    }
}

/// what if a string system call argument is exactly the size
/// of the kernel buffer it is copied into, so that the null
/// would fall just beyond the end of the kernel buffer?
pub fn test2() {
    let mut b = [b'x'; MAX_PATH + 1];
    b[MAX_PATH] = 0;
    let path = CStr::from_bytes_with_nul(&b).unwrap();

    expect!(fs::remove_file(path), Err(Error::Unknown));
    expect!(File::create(path), Err(Error::Unknown));
    expect!(fs::link(path, path), Err(Error::Unknown));

    let args = [c"xx".as_ptr(), ptr::null()];
    expect!(process::exec(path, &args), Err(Error::Unknown));

    let status = process::fork_fn(|| {
        unsafe {
            static mut BIG: [c_char; PAGE_SIZE + 1] = [b'x' as c_char; PAGE_SIZE + 1];
            BIG[PAGE_SIZE] = 0;
            let big = CStr::from_ptr((&raw const BIG).cast());

            let args = [big.as_ptr(), big.as_ptr(), big.as_ptr(), ptr::null()];
            expect!(process::exec(ECHO_PATH, &args), Err(Error::Unknown));
        }
        process::exit(747);
    })
    .unwrap()
    .wait()
    .unwrap();
    assert_eq!(status.code(), 747, "child succeeded");
}

/// what if a string argument crosses over the end of last user page?
pub fn test3() {
    let top = process::grow_break(PAGE_SIZE * 2).unwrap();
    if top.addr() % PAGE_SIZE != 0 {
        process::grow_break(PAGE_SIZE - (top.addr() % PAGE_SIZE)).unwrap();
    }
    let top = process::current_break();
    assert_eq!(top.addr() % PAGE_SIZE, 0, "top is page-aligned");

    unsafe {
        let b = top.wrapping_sub(1);
        *b = b'x';
        let path = { &*(ptr::slice_from_raw_parts(b, 1) as *const CStr) };

        expect!(fs::remove_file(path), Err(Error::Unknown));
        expect!(File::create(path), Err(Error::Unknown));
        expect!(fs::link(path, path), Err(Error::Unknown));

        let args = [c"xx".as_ptr(), ptr::null()];
        expect!(process::exec(path, &args), Err(Error::Unknown));
    }
}
