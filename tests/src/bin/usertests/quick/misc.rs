use core::{cell::UnsafeCell, mem::MaybeUninit, ptr, slice};

use ov6_kernel_params::USER_STACK;
use ov6_user_lib::{
    error::Ov6Error,
    fs::{self, File},
    io::{Read as _, Write as _},
    os_str::OsStr,
    path::Path,
    process,
};

use crate::{BUF, ECHO_PATH, PAGE_SIZE, expect};

pub fn validate() {
    let hi = 1100 * 1024;
    for p in (0..hi).step_by(PAGE_SIZE) {
        // try to crash the kernel by passing a bad string pointer
        unsafe {
            let s = &*{
                ptr::slice_from_raw_parts(ptr::with_exposed_provenance::<u8>(p), 10) as *const Path
            };
            assert!(fs::link("nosuchfile", s).is_err());
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
    const FILE_PATH: &str = "bigarg-ok";

    let _ = fs::remove_file(FILE_PATH);

    let status = process::fork_fn(|| {
        static BIG: &OsStr = OsStr::from_bytes(&[b' '; 400]);
        const ARGS: [&OsStr; 100] = [BIG; 100];
        // this exec() should fail (and return) because the
        // arguments are too large.
        expect!(
            process::exec(ECHO_PATH, &ARGS),
            Err(Ov6Error::ArgumentListTooLong)
        );
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
    'outer: for i in 0.. {
        let name = [
            b'f',
            b'0' + u8::try_from(i / 1000).unwrap(),
            b'0' + u8::try_from((i % 1000) / 100).unwrap(),
            b'0' + u8::try_from((i % 100) / 10).unwrap(),
            b'0' + u8::try_from(i % 10).unwrap(),
        ];
        let path = OsStr::from_bytes(&name);
        let mut file = match File::create(path) {
            Ok(file) => file,
            Err(Ov6Error::StorageFull) => break,
            Err(e) => panic!("unexpected error: {e:?}"),
        };
        nfiles = i + 1;
        loop {
            match file.write(buf) {
                Ok(_) => {}
                Err(Ov6Error::FileTooLarge) => break,
                Err(Ov6Error::StorageFull) => break 'outer,
                Err(e) => panic!("unexpected error: {e:?}"),
            }
        }
        drop(file);
    }

    for n in 0..nfiles {
        let name = [
            b'f',
            b'0' + u8::try_from(n / 1000).unwrap(),
            b'0' + u8::try_from((n % 1000) / 100).unwrap(),
            b'0' + u8::try_from((n % 100) / 10).unwrap(),
            b'0' + u8::try_from(n % 10).unwrap(),
        ];
        let path = OsStr::from_bytes(&name);
        fs::remove_file(path).unwrap();
    }
}

pub fn argp() {
    let mut file = File::open("init").unwrap();
    unsafe {
        let p = slice::from_raw_parts_mut(process::current_break().sub(1), usize::MAX);
        expect!(file.read(p), Err(Ov6Error::BadAddress));
    }
}

/// check that there's an invalid page beneath
/// the user stack, to catch stack overflow.
pub fn stack() {
    #[cfg(target_arch = "riscv64")]
    fn get_sp() -> usize {
        let mut sp: usize;
        unsafe {
            core::arch::asm!("mv {}, sp", out(reg) sp);
        }
        sp
    }
    #[cfg(not(target_arch = "riscv64"))]
    fn get_sp() -> usize {
        panic!("not riscv64");
    }

    let status = process::fork_fn(|| {
        let mut sp = get_sp();
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
        0x8000_0000,
        0x3f_ffff_e000,
        0x3f_ffff_f000,
        0x40_0000_0000,
        0xffff_ffff_ffff_ffff,
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

/// regression test. `copyin()`, `copyout()`, and `copyinstr()` used to cast
/// the virtual page address to uint, which (with certain wild system
/// call arguments) resulted in a kernel page faults.
pub fn pg_bug() {
    let big = ptr::with_exposed_provenance::<u8>(0xeaeb_0b5b_0000_2f5e);
    let argv: &[&str] = &[];
    let path = unsafe { &*(ptr::slice_from_raw_parts(big, 10) as *const OsStr) };
    expect!(process::exec(path, argv), Err(Ov6Error::BadAddress));
}

/// regression test. does the kernel panic if a process `sbrk()`s its
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
/// still `copyin()` from addresses in the last page?
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
        let path = &*(ptr::slice_from_raw_parts(p, 1) as *const Path);
        let mut file = File::create(path).unwrap();
        file.write(slice::from_raw_parts(p, 1)).unwrap();
        drop(file);

        let mut file = File::open(path).unwrap();
        file.read(slice::from_raw_parts_mut(p, 1)).unwrap();
        assert_eq!(p.add(0).read(), b'x');
    }

    fs::remove_file("x").unwrap();
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

/// regression test. test whether `exec()` leaks memory if one of the
/// arguments is invalid. the test passes if the kernel doesn't panic.
pub fn bad_arg() {
    for _ in 0..50000 {
        let argv = [unsafe {
            &*(ptr::slice_from_raw_parts::<u8>(ptr::with_exposed_provenance(0xffff_ffff), 1)
                as *const OsStr)
        }];
        expect!(process::exec(ECHO_PATH, &argv), Err(Ov6Error::BadAddress));
    }
}
