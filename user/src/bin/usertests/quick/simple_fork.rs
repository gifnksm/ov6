use core::{alloc::Layout, ffi::CStr, hint, ptr::NonNull};

use alloc::alloc::{Allocator as _, Global};
use ov6_user_lib::{
    eprint,
    fs::{self, File},
    io::{Read as _, Write as _},
    pipe, process, thread,
};

use crate::BUF;

pub fn pipe() {
    const N: usize = 5;
    const SIZE: usize = 1033;

    let buf = unsafe { (&raw mut BUF).as_mut() }.unwrap();

    let (mut rx, mut tx) = pipe::pipe().unwrap();
    if process::fork().unwrap().is_child() {
        drop(rx);
        let mut seq = 0;
        for _n in 0..N {
            for b in &mut buf[0..SIZE] {
                *b = seq;
                seq = seq.wrapping_add(1);
            }
            tx.write(&buf[..SIZE]).unwrap();
        }
        process::exit(0);
    }

    drop(tx);
    let mut total = 0;
    let mut cc = 1;
    let mut seq = 0;
    loop {
        let n = rx.read(&mut buf[..cc]).unwrap();
        if n == 0 {
            break;
        }
        for b in &mut buf[0..n] {
            assert_eq!(*b, seq, "n={n}, cc={cc}");
            seq = seq.wrapping_add(1);
        }
        total += n;
        cc = usize::min(cc * 2, buf.len());
    }
    assert_eq!(total, N * SIZE);
    let (_, status) = process::wait().unwrap();
    assert!(status.success());
}

/// test if child is killed (status = -1)
pub fn kill_status() {
    for _ in 0..100 {
        let child = process::fork_fn(|| {
            loop {
                let _ = process::id();
            }
        })
        .unwrap();
        thread::sleep(1);
        process::kill(child.pid()).unwrap();
        let status = child.wait().unwrap();
        assert_eq!(status.code(), -1);
    }
}

/// meant to be run w/ at most two CPUs
pub fn preempt() {
    let buf = unsafe { (&raw mut BUF).as_mut() }.unwrap();

    let child1 = process::fork_fn(|| {
        loop {
            hint::spin_loop()
        }
    })
    .unwrap();
    let child2 = process::fork_fn(|| {
        loop {
            hint::spin_loop()
        }
    })
    .unwrap();

    let (mut rx, mut tx) = pipe::pipe().unwrap();
    let Some(pid3) = process::fork().unwrap().as_parent() else {
        drop(rx);
        tx.write_all(b"x").unwrap();
        drop(tx);
        loop {
            hint::spin_loop();
        }
    };

    drop(tx);
    rx.read(buf).unwrap();
    drop(rx);
    eprint!("kill... ");
    process::kill(child1.pid()).unwrap();
    process::kill(child2.pid()).unwrap();
    process::kill(pid3).unwrap();
    eprint!("wait... ");
    process::wait().unwrap();
    process::wait().unwrap();
    process::wait().unwrap();
}

/// try to find any races between exit and wait
pub fn exit_wait() {
    for i in 0..100 {
        let status = process::fork_fn(|| {
            process::exit(i);
        })
        .unwrap()
        .wait()
        .unwrap();
        assert_eq!(status.code(), i);
    }
}

/// try to find races in the reparenting
/// code that handles a parent exiting
/// when it still has live children.
pub fn reparent1() {
    let master_pid = process::id();

    for _i in 0..200 {
        process::fork_fn(|| {
            process::fork()
                .inspect_err(|_| {
                    process::kill(master_pid).unwrap();
                })
                .unwrap();
            process::exit(0);
        })
        .unwrap()
        .wait()
        .unwrap();
    }
}

/// what if two children exit() at the same time?
pub fn two_children() {
    for _i in 0..1000 {
        let _child1 = process::fork_fn(|| process::exit(0)).unwrap();
        let _child2 = process::fork_fn(|| process::exit(0)).unwrap();
        process::wait().unwrap();
        process::wait().unwrap();
    }
}

/// concurrent forks to try to expose locking bugs.
pub fn fork_fork() {
    const N: usize = 2;

    for _ in 0..N {
        process::fork_fn(|| {
            for _ in 0..200 {
                process::fork_fn(|| process::exit(0))
                    .unwrap()
                    .wait()
                    .unwrap();
            }
            process::exit(0);
        })
        .unwrap();
    }

    for _ in 0..N {
        let (_, status) = process::wait().unwrap();
        assert!(status.success());
    }
}

pub fn fork_fork_fork() {
    const STOP_FORKING_PATH: &CStr = c"stopforking";

    let _ = fs::remove_file(STOP_FORKING_PATH);

    let child = process::fork_fn(|| {
        loop {
            if File::open(STOP_FORKING_PATH).is_ok() {
                process::exit(0);
            }
            if process::fork().is_err() {
                let _ = File::create(STOP_FORKING_PATH).unwrap();
            }
        }
    })
    .unwrap();
    thread::sleep(20); // two seconds
    let _ = File::create(STOP_FORKING_PATH).unwrap();
    child.wait().unwrap();
    thread::sleep(10); // one second
}

/// regression test. does reparent() violate the parent-then-child
/// locking order when giving away a child to init, so that exit()
/// deadlocks against init's wait()? also used to trigger a "panic:
/// release" due to exit() releasing a different p->parent->lock than
/// it acquired.
pub fn reparent2() {
    for _ in 0..800 {
        let child = process::fork_fn(|| {
            process::fork().unwrap();
            process::fork().unwrap();
            process::exit(0);
        })
        .unwrap();
        child.wait().unwrap();
    }
}

/// allocate all mem, free it, and allocate again
pub fn mem() {
    let status = process::fork_fn(|| unsafe {
        struct Ptr(Option<NonNull<Ptr>>);
        let layout = Layout::from_size_align(10001, 1).unwrap();
        let mut m1: Option<NonNull<Ptr>> = None;
        while let Ok(m2) = Global.allocate(layout) {
            m2.cast().write(m1);
            m1 = Some(m2.cast());
        }
        while let Some(p) = m1 {
            let m2 = p.read().0;
            Global.deallocate(p.cast(), layout);
            m1 = m2;
        }
        let layout = Layout::from_size_align(1024, 1).unwrap();
        let m1 = Global.allocate(layout).unwrap();
        Global.deallocate(m1.cast(), layout);
        process::exit(0);
    })
    .unwrap()
    .wait()
    .unwrap();
    assert!(status.success());
}
