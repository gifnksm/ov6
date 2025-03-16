use alloc::slice;
use core::{ptr, time::Duration};

use ov6_user_lib::{
    error::Ov6Error,
    fs::{self, File},
    io::{Read as _, Write as _},
    pipe, process, thread,
};
use ov6_user_tests::message;

use crate::{KERN_BASE, MAX_VA, PAGE_SIZE, expect};

/// test that fork fails gracefully
/// the forktest binary also does this, but it runs out of proc entries first.
/// inside the bigger usertests binary, we run out of memory first.
pub fn fork() {
    const N: usize = 1000;

    let mut n = 0;
    for _ in 0..N {
        if process::fork_fn(|| process::exit(0)).is_err() {
            break;
        }
        n += 1;
    }
    assert_ne!(n, 0, "no fork at all!");
    assert_ne!(n, N, "fork claimed to work {N} times!");

    for _ in 0..n {
        process::wait().unwrap();
    }

    expect!(process::wait(), Err(Ov6Error::NoChildProcess));
}

pub fn sbrk_basic() {
    const TOO_MUCH: usize = 1024 * 1024 * 1024;

    // does sbrk() return the sexpected failure value?
    let status = process::fork_fn(|| {
        let Ok(a) = process::grow_break(TOO_MUCH).map_err(|e| {
            assert!(matches!(e, Ov6Error::OutOfMemory));
            process::exit(0);
        });
        unsafe {
            for n in (0..TOO_MUCH).step_by(4096) {
                a.add(n).write(99);
            }
        }

        // we should not get here! either sbrk(TOOMUCH)
        // should have failed, or (with lazy allocation)
        // a pagefault should have killed this process.
        unreachable!();
    })
    .unwrap()
    .wait()
    .unwrap();
    assert!(status.success());

    // can one sbrk() less than a page?
    let mut a = process::current_break();
    for _ in 0..5000 {
        let b = process::grow_break(1).unwrap();
        assert_eq!(b, a);
        a = b.wrapping_add(1);
    }
    let res = process::fork().unwrap();
    process::grow_break(1).unwrap();
    let c = process::grow_break(1).unwrap();
    assert_eq!(c, a.wrapping_add(1));
    if res.is_child() {
        process::exit(0);
    }
    let (_, status) = process::wait().unwrap();
    assert!(status.success());
}

pub fn sbrk_much() {
    const BIG: usize = 100 * 1024 * 1024;

    let old_break = process::current_break();

    // can one grow address space to soemething big?;
    let a = process::current_break();
    let amt = BIG - a.addr();
    let p = process::grow_break(amt).unwrap();
    assert_eq!(p, a);

    // touch each page to make sure it exists
    let eee = process::current_break();
    for pp in (a.addr()..eee.addr()).step_by(4096) {
        unsafe {
            ptr::with_exposed_provenance_mut::<u8>(pp).write(99);
        }
    }

    let last_addr = ptr::with_exposed_provenance_mut::<u8>(BIG - 1);
    unsafe {
        last_addr.write(99);
    }

    // can one de-allocate?
    let a = process::current_break();
    unsafe {
        process::shrink_break(PAGE_SIZE).unwrap();
    }
    let c = process::current_break();
    assert_eq!(c, a.wrapping_sub(PAGE_SIZE));

    // can one re-allocate that page?
    let a = process::current_break();
    let c = process::grow_break(PAGE_SIZE).unwrap();
    assert_eq!(a, c);
    assert_eq!(process::current_break(), a.wrapping_add(PAGE_SIZE));

    unsafe {
        assert_eq!(last_addr.read(), 0);
    }

    let a = process::current_break();
    let c = unsafe { process::shrink_break(a.addr() - old_break.addr()) }.unwrap();
    assert_eq!(a, c);
}

/// can we read the kernel's memory?
pub fn kern_mem() {
    for i in (0..2_000_000).step_by(50_000) {
        let a = ptr::with_exposed_provenance_mut::<u8>(KERN_BASE + i);

        let status = process::fork_fn(|| {
            let v = unsafe { a.read() };
            message!("oops could read {a:p} = {v}");
            process::exit(0);
        })
        .unwrap()
        .wait()
        .unwrap();
        assert_eq!(status.code(), -1);
    }
}

/// user code should not be able to write to addresses above MAXVA.
pub fn max_va_plus() {
    let mut a = MAX_VA;
    loop {
        let status = process::fork_fn(|| {
            unsafe {
                ptr::with_exposed_provenance_mut::<u8>(a).write(99);
            }
            message!("oops wrote {a}");
            process::exit(1);
        })
        .unwrap()
        .wait()
        .unwrap();
        assert_eq!(status.code(), -1);

        a <<= 1;
        if a == 0 {
            break;
        }
    }
}

/// if we run the system out of memory, does it clean up the last
/// failed allocation?
pub fn sbrk_fail() {
    const BIG: usize = 100 * 1024 * 1024;

    let (mut rx, mut tx) = pipe::pipe().unwrap();
    let mut pids = [None; 10];

    for pid in &mut pids {
        let Some(p) = process::fork().unwrap().as_parent() else {
            // allocate a lot of memory
            let _ = process::grow_break(BIG - process::current_break().addr());
            tx.write(b"x").unwrap();
            loop {
                thread::sleep(Duration::from_secs(100));
            }
        };
        *pid = Some(p);
        rx.read_exact(&mut [0]).unwrap();
    }

    // if those failed allocations freed up the pages they did allocate,
    // we'll be able to allocate here
    process::grow_break(PAGE_SIZE).unwrap();
    for &pid in pids.iter().flatten() {
        process::kill(pid).unwrap();
        process::wait().unwrap();
    }

    // test running fork with the above allocated page
    let status = process::fork_fn(|| {
        let a = process::current_break();
        let _ = process::grow_break(10 * BIG);
        let mut n = 0;
        for i in (0..10 * BIG).step_by(PAGE_SIZE) {
            n += unsafe { a.add(i).read() };
        }
        // print n so the compiler doesn't optimize away
        // the for loop.
        panic!("allocate a lot of memory succeeded {n}");
    })
    .unwrap()
    .wait()
    .unwrap();
    assert!(status.success() || status.code() == -1);
}

/// test reads/writes from/to allocated memory
pub fn sbrk_arg() {
    let a = process::grow_break(PAGE_SIZE).unwrap();
    let mut file = File::create("sbrk").unwrap();
    fs::remove_file("sbrk").unwrap();
    file.write_all(unsafe { slice::from_raw_parts(a, PAGE_SIZE) })
        .unwrap();
    drop(file);

    // test writes to allocated memory
    let a = process::grow_break(PAGE_SIZE).unwrap();
    unsafe {
        a.write(0);
    }
}
