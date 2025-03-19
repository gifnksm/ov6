use alloc::alloc::Global;
use core::{
    alloc::{Allocator as _, Layout},
    hint,
    num::NonZero,
    ptr::NonNull,
    time::Duration,
};

use ov6_user_lib::{
    eprint,
    error::Ov6Error,
    fs::{self, File},
    io::{self, Read as _, Write as _},
    os::{fd::AsRawFd as _, ov6::syscall},
    pipe,
    process::{self, ProcId, ProcessBuilder, Stdio},
    thread,
};

use crate::{BUF, expect};

pub fn pipe() {
    const N: usize = 5;
    const SIZE: usize = 1033;

    let buf = unsafe { (&raw mut BUF).as_mut() }.unwrap();

    let mut child = ProcessBuilder::new()
        .stdout(Stdio::Pipe)
        .spawn_fn(|| {
            let mut seq = 0;
            for _n in 0..N {
                for b in &mut buf[0..SIZE] {
                    *b = seq;
                    seq = seq.wrapping_add(1);
                }
                io::stdout().write_all(&buf[..SIZE]).unwrap();
            }
            process::exit(0);
        })
        .unwrap();

    let mut rx = child.stdout.take().unwrap();
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
    assert!(child.wait().unwrap().success());
}

pub fn broken_pipe() {
    let (rx, mut tx) = pipe::pipe().unwrap();
    drop(rx);
    expect!(tx.write_all(&[1, 2, 3]), Err(Ov6Error::BrokenPipe));
}

pub fn pipe_bad_fd() {
    let (rx, tx) = pipe::pipe().unwrap();
    expect!(
        syscall::write(rx.as_raw_fd(), &[0]),
        Err(Ov6Error::BadFileDescriptor)
    );
    expect!(
        syscall::read(tx.as_raw_fd(), &mut [0]),
        Err(Ov6Error::BadFileDescriptor)
    );
}

/// test if child is killed (status = -1)
pub fn kill_status() {
    for _ in 0..100 {
        let mut child = ProcessBuilder::new()
            .spawn_fn(|| {
                loop {
                    let _ = process::id();
                }
            })
            .unwrap();
        thread::sleep(Duration::from_millis(100));
        child.kill().unwrap();
        let status = child.wait().unwrap();
        assert_eq!(status.code(), -1);
    }
}

pub fn kill_error() {
    expect!(
        process::kill(ProcId::new(NonZero::<u32>::MAX)),
        Err(Ov6Error::ProcessNotFound),
    );
}

/// meant to be run w/ at most two CPUs
pub fn preempt() {
    let mut child1 = ProcessBuilder::new()
        .spawn_fn(|| {
            loop {
                hint::spin_loop();
            }
        })
        .unwrap();
    let mut child2 = ProcessBuilder::new()
        .spawn_fn(|| {
            loop {
                hint::spin_loop();
            }
        })
        .unwrap();

    let mut child3 = ProcessBuilder::new()
        .stdout(Stdio::Pipe)
        .spawn_fn(|| {
            io::stdout().write_all(b"x").unwrap();
            loop {
                hint::spin_loop();
            }
        })
        .unwrap();

    child3.stdout.take().unwrap().read_exact(&mut [0]).unwrap();
    eprint!("kill... ");
    child1.kill().unwrap();
    child2.kill().unwrap();
    child3.kill().unwrap();
    eprint!("wait... ");
    child1.wait().unwrap();
    child2.wait().unwrap();
    child3.wait().unwrap();
}

/// try to find any races between exit and wait
pub fn exit_wait() {
    for i in 0..100 {
        let status = ProcessBuilder::new()
            .spawn_fn(|| {
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
        ProcessBuilder::new()
            .spawn_fn(|| {
                ProcessBuilder::new()
                    .spawn_fn(|| {
                        process::exit(0);
                    })
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

/// what if two children `exit()` at the same time?
pub fn two_children() {
    for _i in 0..1000 {
        let mut child1 = ProcessBuilder::new().spawn_fn(|| process::exit(0)).unwrap();
        let mut child2 = ProcessBuilder::new().spawn_fn(|| process::exit(0)).unwrap();
        assert!(child1.wait().unwrap().success());
        assert!(child2.wait().unwrap().success());
    }
}

/// concurrent forks to try to expose locking bugs.
pub fn fork_fork() {
    const N: usize = 2;

    for _ in 0..N {
        ProcessBuilder::new()
            .spawn_fn(|| {
                for _ in 0..200 {
                    ProcessBuilder::new()
                        .spawn_fn(|| process::exit(0))
                        .unwrap()
                        .wait()
                        .unwrap();
                }
                process::exit(0);
            })
            .unwrap();
    }

    for _ in 0..N {
        let (_, status) = process::wait_any().unwrap();
        assert!(status.success());
    }
}

pub fn fork_fork_fork() {
    const STOP_FORKING_PATH: &str = "stopforking";

    let _ = fs::remove_file(STOP_FORKING_PATH);

    let mut child = ProcessBuilder::new()
        .spawn_fn(|| {
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
    thread::sleep(Duration::from_secs(2));
    let _ = File::create(STOP_FORKING_PATH).unwrap();
    child.wait().unwrap();
    thread::sleep(Duration::from_secs(1));
}

/// regression test. does `reparent()` violate the parent-then-child
/// locking order when giving away a child to init, so that `exit()`
/// deadlocks against init's `wait()`? also used to trigger a "panic:
/// release" due to `exit()` releasing a different p->parent->lock than
/// it acquired.
pub fn reparent2() {
    for _ in 0..800 {
        let mut child = ProcessBuilder::new()
            .spawn_fn(|| {
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
    let status = ProcessBuilder::new()
        .spawn_fn(|| unsafe {
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
