#![feature(allocator_api)]
#![cfg_attr(not(test), no_std)]

use core::{array, ptr, time::Duration};

use ov6_user_lib::{
    io::{self, Read as _, Write as _},
    os::ov6::syscall,
    process::{self, ProcessBuilder, Stdio},
    thread,
};
use ov6_user_tests::test_runner::{TestEntry, TestParam};

fn main() {
    TestParam::parse().run(TESTS);
}

const TESTS: &[TestEntry] = &[
    TestEntry {
        name: "simple",
        test: simple,
        tags: &[],
    },
    TestEntry {
        name: "simple_twice",
        test: simple_twice,
        tags: &[],
    },
    TestEntry {
        name: "three",
        test: three,
        tags: &[],
    },
    TestEntry {
        name: "three_3times",
        test: three_3times,
        tags: &[],
    },
    TestEntry {
        name: "file",
        test: file,
        tags: &[],
    },
    TestEntry {
        name: "fork_fork",
        test: fork_fork,
        tags: &[],
    },
];

/// Allocate more than half of physical memory,
/// then fork.
///
/// This will fail in the default
/// kernel, which does not support copy-on-write.
fn simple() {
    let sysinfo = syscall::get_system_info().unwrap();
    let meminfo = sysinfo.memory;
    let mem = meminfo.total_pages * meminfo.page_size;
    let size = (mem / 3) * 2;

    let p = process::grow_break(size).unwrap();

    for i in (0..size).step_by(meminfo.page_size) {
        unsafe {
            let addr = p.add(i);
            let bytes = &process::id().get().get().to_ne_bytes();
            ptr::copy(bytes.as_ptr(), addr, bytes.len());
        }
    }

    let status = ProcessBuilder::new()
        .spawn_fn(|| {
            process::exit(0);
        })
        .unwrap()
        .wait()
        .unwrap();

    assert!(status.success());

    unsafe { process::shrink_break(size) }.unwrap();
}

/// Check that `simple` freed the physical memory.
fn simple_twice() {
    simple();
    simple();
}

/// Three processes all write COW memory.
///
/// This causes more than half of physical memory
/// to be allocated, so it also checks whether
/// copied pages are freed.
fn three() {
    let sysinfo = syscall::get_system_info().unwrap();
    let meminfo = sysinfo.memory;
    let mem = meminfo.total_pages * meminfo.page_size;
    let size = mem / 4;

    let p = process::grow_break(size).unwrap();

    let mut child = ProcessBuilder::new()
        .spawn_fn(|| {
            let mut child = ProcessBuilder::new()
                .spawn_fn(|| {
                    for i in (0..(size / 5) * 4).step_by(meminfo.page_size) {
                        unsafe {
                            let addr = p.add(i);
                            let bytes = &process::id().get().get().to_ne_bytes();
                            ptr::copy(bytes.as_ptr(), addr, bytes.len());
                        }
                    }
                    for i in (0..(size / 5) * 4).step_by(meminfo.page_size) {
                        unsafe {
                            let addr = p.add(i);
                            let mut bytes = [0; 4];
                            ptr::copy(addr, bytes.as_mut_ptr(), bytes.len());
                            assert_eq!(u32::from_ne_bytes(bytes), process::id().get().get());
                        }
                    }
                    process::exit(0);
                })
                .unwrap();
            for i in (0..size / 2).step_by(meminfo.page_size) {
                unsafe {
                    let addr = p.add(i);
                    let bytes = 9999_u32.to_ne_bytes();
                    ptr::copy(bytes.as_ptr(), addr, bytes.len());
                }
            }
            assert!(child.wait().unwrap().success());
            process::exit(0);
        })
        .unwrap();

    for i in (0..size).step_by(meminfo.page_size) {
        unsafe {
            let addr = p.add(i);
            let bytes = &process::id().get().get().to_ne_bytes();
            ptr::copy(bytes.as_ptr(), addr, bytes.len());
        }
    }

    assert!(child.wait().unwrap().success());

    thread::sleep(Duration::from_millis(10));

    for i in (0..size).step_by(meminfo.page_size) {
        unsafe {
            let addr = p.add(i);
            let mut bytes = [0; 4];
            ptr::copy(addr, bytes.as_mut_ptr(), bytes.len());
            assert_eq!(u32::from_ne_bytes(bytes), process::id().get().get());
        }
    }

    unsafe { process::shrink_break(size) }.unwrap();
}

fn three_3times() {
    three();
    three();
    three();
}

static mut BUF: [u8; 4096] = [0; 4096];

/// test whether copy_k2u simulates COW faults.
fn file() {
    let buf = unsafe { (&raw mut BUF).as_mut().unwrap() };

    buf[0] = 99;

    let children: [_; 4] = array::from_fn(|i| {
        let mut child = ProcessBuilder::new()
            .stdin(Stdio::Pipe)
            .spawn_fn(|| {
                thread::sleep(Duration::from_millis(10));
                io::stdin().read_exact(&mut buf[..8]).unwrap();
                thread::sleep(Duration::from_millis(10));
                assert_eq!(usize::from_ne_bytes(buf[..8].try_into().unwrap()), i);
                process::exit(0);
            })
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(&i.to_ne_bytes()).unwrap();
        child
    });
    for mut child in children {
        assert!(child.wait().unwrap().success());
    }

    assert_eq!(buf[0], 99);
}

/// Try to expose races in page reference counting
fn fork_fork() {
    let size = 256 * 4096;
    let p = process::grow_break(size).unwrap();
    unsafe {
        p.write_bytes(27, size);
    }

    for _i in 0..100 {
        let children: [_; 3] = array::from_fn(|_| {
            ProcessBuilder::new()
                .spawn_fn(|| {
                    thread::sleep(Duration::from_millis(20));
                    let _ = process::fork().unwrap();
                    let _ = process::fork().unwrap();
                    process::exit(0);
                })
                .unwrap()
        });

        for mut child in children {
            assert!(child.wait().unwrap().success());
        }
        ov6_user_lib::eprint!(".");
    }

    thread::sleep(Duration::from_millis(50));
    for i in (0..size).step_by(4096) {
        unsafe {
            assert_eq!(*p.add(i), 27);
        }
    }
}
