use core::{
    arch::asm,
    cell::UnsafeCell,
    ffi::{CStr, c_char},
    mem::MaybeUninit,
    ptr, slice,
};

use xv6_kernel_params::{MAX_ARG, USER_STACK};
use xv6_user_lib::{
    error::Error,
    fs::{self, File},
    io::{Read as _, Write as _},
    process,
};

use crate::{BUF, ECHO_PATH, PAGE_SIZE, expect};

pub fn validate() {
    let hi = 1100 * 1024;
    for p in (0..hi).step_by(PAGE_SIZE) {
        // try to crash the kernel by passing a bad string pointer
        unsafe {
            let s = &*{
                ptr::slice_from_raw_parts(ptr::with_exposed_provenance::<u8>(p), 10) as *const CStr
            };
            expect!(fs::link(c"nosuchfile", s), Err(Error::Unknown));
        }
    }
}

/// does uninitialized data start out zero?
pub fn bss() {
    struct X(UnsafeCell<MaybeUninit<[u8; 10000]>>);
    unsafe impl Sync for X {}
    #[unsafe(export_name = "UNINIT")]
    static UNINIT: X = X(UnsafeCell::new(MaybeUninit::uninit()));

    unsafe {
        let uninit = UNINIT.0.get().as_mut().unwrap().assume_init_ref();
        for &i in uninit {
            assert_eq!(i, 0);
        }
    }
}

/// does exec return an error if the arguments
/// are larger than a page? or does it write
/// below the stack and wreck the instructions/data?
pub fn big_arg() {
    const FILE_PATH: &CStr = c"bigarg-ok";

    let _ = fs::remove_file(FILE_PATH);
    let status = process::fork_fn(|| {
        static mut ARGS: [*const c_char; MAX_ARG] = [ptr::null(); MAX_ARG];
        let args = unsafe { (&raw mut ARGS).as_mut().unwrap() };
        let mut big = [b' ' as c_char; 400];
        *big.last_mut().unwrap() = b'\0' as c_char;
        for arg in &mut args[..MAX_ARG - 1] {
            *arg = big.as_ptr();
        }
        args[MAX_ARG - 1] = ptr::null();
        // this exec() should fail (and return) because the
        // arguments are too large.
        expect!(process::exec(FILE_PATH, args), Err(Error::Unknown));
        let _ = File::create(FILE_PATH).unwrap();
        process::exit(0);
    })
    .unwrap()
    .wait()
    .unwrap();
    assert!(status.success());

    let _ = File::open(FILE_PATH).unwrap();
    fs::remove_file(FILE_PATH).unwrap();
}

/// what happens when the file system runs out of blocks?
pub fn fs_full() {
    let buf = unsafe { (&raw mut BUF).as_mut() }.unwrap();
    let mut nfiles = 0;
    for n in 0.. {
        let mut name = [0u8; 6];
        name[0] = b'f';
        name[1] = b'0' + u8::try_from(n / 1000).unwrap();
        name[2] = b'0' + u8::try_from((n % 1000) / 100).unwrap();
        name[3] = b'0' + u8::try_from((n % 100) / 10).unwrap();
        name[4] = b'0' + u8::try_from(n % 10).unwrap();
        name[5] = b'\0';
        let path = CStr::from_bytes_with_nul(&name).unwrap();
        let Ok(mut file) = File::create(path) else {
            nfiles = n;
            break;
        };
        let mut total = 0;
        loop {
            let Ok(n) = file.write(buf) else {
                nfiles = n;
                break;
            };
            total += n;
        }
        drop(file);
        if total == 0 {
            nfiles += 1;
            break;
        }
    }

    for n in 0..nfiles {
        let mut name = [0u8; 6];
        name[0] = b'f';
        name[1] = b'0' + u8::try_from(n / 1000).unwrap();
        name[2] = b'0' + u8::try_from((n % 1000) / 100).unwrap();
        name[3] = b'0' + u8::try_from((n % 100) / 10).unwrap();
        name[4] = b'0' + u8::try_from(n % 10).unwrap();
        name[5] = b'\0';
        let path = CStr::from_bytes_with_nul(&name).unwrap();
        fs::remove_file(path).unwrap();
    }
}

pub fn argp() {
    let mut file = File::open(c"init").unwrap();
    unsafe {
        let p = slice::from_raw_parts_mut(process::current_break().sub(1), usize::MAX);
        expect!(file.read(p), Err(Error::Unknown));
    }
}

/// check that there's an invalid page beneath
/// the user stack, to catch stack overflow.
pub fn stack() {
    let status = process::fork_fn(|| {
        let mut sp: usize;
        unsafe {
            asm!("mv {}, sp", out(reg) sp);
        }
        sp -= USER_STACK * PAGE_SIZE;
        // the sp should cause a trap
        let v = unsafe { ptr::with_exposed_provenance::<u8>(sp).read() };
        unreachable!("read below stack: {v}");
    })
    .unwrap()
    .wait()
    .unwrap();
    assert_eq!(status.code(), -1);
}

/// check that writes to a few forbidden addresses
/// cause a fault, e.g. process's text and TRAMPOLINE.
pub fn no_write() {
    let addrs = &[
        0,
        0x80000000,
        0x3fffffe000,
        0x3ffffff000,
        0x4000000000,
        0xffffffffffffffff,
    ];

    for &a in addrs {
        let status = process::fork_fn(|| unsafe {
            let p = ptr::with_exposed_provenance_mut::<u8>(a);
            p.write_volatile(10);
            panic!("write to {p:p} did not fail!");
        })
        .unwrap()
        .wait()
        .unwrap();
        assert_eq!(status.code(), -1);
    }
}

/// regression test. copyin(), copyout(), and copyinstr() used to cast
/// the virtual page address to uint, which (with certain wild system
/// call arguments) resulted in a kernel page faults.
pub fn pg_bug() {
    let big = ptr::with_exposed_provenance::<u8>(0xeaeb0b5b00002f5e);
    let argv = &[ptr::null()];
    let path = unsafe { &*(ptr::slice_from_raw_parts(big, 10) as *const CStr) };
    expect!(process::exec(path, argv), Err(Error::Unknown));
}

/// regression test. does the kernel panic if a process sbrk()s its
/// size to be less than a page, or zero, or reduces the break by an
/// amount too small to cause a page to be freed?
pub fn sbrk_bugs() {
    let status = process::fork_fn(|| {
        let sz = process::current_break().addr();
        // free all user memory; there used to be a bug that
        // would not adjust p->sz correctly in this case,
        // causing exit() to panic.
        unsafe { process::shrink_break(sz) }.unwrap();
        process::exit(0);
    })
    .unwrap()
    .wait()
    .unwrap();
    assert_eq!(status.code(), -1);

    let status = process::fork_fn(|| {
        let sz = process::current_break().addr();
        // set the break to somewhere in the very first
        // page; there used to be a bug that would incorrectly
        // free the first page.
        unsafe { process::shrink_break(sz - 3500) }.unwrap();
        process::exit(0);
    })
    .unwrap()
    .wait()
    .unwrap();
    assert_eq!(status.code(), -1);

    let status = process::fork_fn(|| {
        // set the break in the middle of a page.
        process::grow_break(usize::abs_diff(
            process::current_break().addr(),
            10 * PAGE_SIZE + PAGE_SIZE / 2,
        ))
        .unwrap();

        // reduce the break a bit, but not enough to
        // cause a page to be freed. this used to cause
        // a panic.
        unsafe { process::shrink_break(10) }.unwrap();
        process::exit(0);
    })
    .unwrap()
    .wait()
    .unwrap();
    assert!(status.success());
}

/// if process size was somewhat more than a page boundary, and then
/// shrunk to be somewhat less than that page boundary, can the kernel
/// still copyin() from addresses in the last page?
pub fn sbrk_last() {
    let top = process::current_break().addr();
    if (top % PAGE_SIZE) != 0 {
        process::grow_break(PAGE_SIZE - (top % PAGE_SIZE)).unwrap();
    }
    process::grow_break(PAGE_SIZE).unwrap();
    process::grow_break(10).unwrap();
    unsafe { process::shrink_break(20) }.unwrap();
    let top = process::current_break();
    unsafe {
        let p = top.sub(64);
        p.add(0).write(b'x');
        p.add(1).write(b'\0');
        let path = &*(ptr::slice_from_raw_parts(p, 2) as *const CStr);
        let mut file = File::create(path).unwrap();
        file.write(slice::from_raw_parts(p, 1)).unwrap();
        drop(file);

        let mut file = File::open(path).unwrap();
        file.read(slice::from_raw_parts_mut(p, 1)).unwrap();
        assert_eq!(p.add(0).read(), b'x');
    }
}

/// does sbrk handle signed int32 wrap-around with
/// negative arguments?
pub fn sbrk8000() {
    let _ = process::grow_break(0x8000_0004);
    let top = process::current_break();
    unsafe {
        top.sub(1).write_volatile(top.sub(1).read_volatile() + 1);
    }
}

/// regression test. test whether exec() leaks memory if one of the
/// arguments is invalid. the test passes if the kernel doesn't panic.
pub fn bad_arg() {
    for _ in 0..50000 {
        let argv = [ptr::with_exposed_provenance(0xffff_ffff), ptr::null()];
        expect!(process::exec(ECHO_PATH, &argv), Err(Error::Unknown));
    }
}
