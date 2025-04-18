#![cfg_attr(not(test), no_std)]

//! # Overview
//!
//! This program is a stress test for various system calls and file operations.
//! It performs a wide range of operations, including file creation, deletion,
//! directory navigation, process management, and inter-process communication.
//!
//! # Note
//!
//! The use of `io::stdout()` or macros like `print!` is prohibited in this
//! test. This is because the internal `BufWriter` of `Stdout` uses heap memory,
//! and this test involves growing and shrinking the heap area. Using
//! `io::stdout()` could interfere with the heap operations being tested.

use core::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use ov6_user_lib::{
    env,
    fs::{self, File},
    io::{Read as _, STDOUT_FD, Write as _},
    os::ov6::syscall,
    pipe,
    process::{self, ProcessBuilder, Stdio},
    thread,
};

static RAND_NEXT: AtomicU64 = AtomicU64::new(1);

fn do_rand(ctx: &mut u64) -> u64 {
    // Compute x = (7^5 * x) mod (2^31 - 1)
    // without overflowing 31 bits:
    //     (2^31 - 1) = 127773 * (7^5) + 2836
    // From "Random number generators: good ones are hard to find",
    // Park and Miller, Communications of the ACM, vol. 31, no. 10,
    // October 1988, p. 1195.

    // Transform to [1, 0x7ffffffe] range
    let x = (*ctx % 0x7fff_fffe) + 1;
    let hi = x / 127_773;
    let lo = x % 127_773;
    let x = u64::wrapping_sub(16807 * lo, 2836 * hi);

    // Transform to [0, 0x7ffffffd] range
    let x = x - 1;

    *ctx = x;
    x
}

fn rand() -> u64 {
    let mut ctx = RAND_NEXT.load(Ordering::Relaxed);
    let n = do_rand(&mut ctx);
    RAND_NEXT.store(ctx, Ordering::Relaxed);
    n
}

#[expect(clippy::too_many_lines)]
fn go(name: u8) {
    let mut buf = [0; 999];
    let break0 = process::current_break().addr();

    let _ = fs::create_dir("grindir");
    env::set_current_directory("grindir").unwrap();
    env::set_current_directory("/").unwrap();

    let mut file = None;

    for iters in 1.. {
        if iters % 500 == 0 {
            // We cannot use `print!` here. See module-level documentation.
            syscall::write(STDOUT_FD, &[name]).unwrap();
        }

        match rand() % 23 {
            0 => {}
            1 => {
                let _ = File::options()
                    .read(true)
                    .write(true)
                    .create(true)
                    .open("grindir/../a");
            }
            2 => {
                let _ = File::options()
                    .read(true)
                    .write(true)
                    .create(true)
                    .open("grindir/../grindir/../b");
            }
            3 => {
                let _ = fs::remove_file("grindir/../a");
            }
            4 => {
                env::set_current_directory("grindir").unwrap();
                let _ = fs::remove_file("../b");
                env::set_current_directory("/").unwrap();
            }
            5 => {
                let _ = file.take();
                file = File::options()
                    .read(true)
                    .write(true)
                    .create(true)
                    .open("/grindir/../a")
                    .ok();
            }
            6 => {
                let _ = file.take();
                file = File::options()
                    .read(true)
                    .write(true)
                    .create(true)
                    .open("/./grindir/./../b")
                    .ok();
            }
            7 => {
                if let Some(file) = &mut file {
                    let _ = file.write(&buf);
                }
            }
            8 => {
                if let Some(file) = &mut file {
                    let _ = file.read(&mut buf);
                }
            }
            9 => {
                let _ = fs::create_dir("grindir/../a");
                let _ = File::options()
                    .read(true)
                    .write(true)
                    .create(true)
                    .open("a/../a/./a");
                let _ = fs::remove_file("a/a");
            }
            10 => {
                let _ = fs::create_dir("/../b");
                let _ = File::options()
                    .read(true)
                    .write(true)
                    .create(true)
                    .open("grindir/../b/b");
                let _ = fs::remove_file("b/b");
            }
            11 => {
                let _ = fs::remove_file("b");
                let _ = fs::link("../grindir/./../a", "../b");
            }
            12 => {
                let _ = fs::remove_file("../grindir/../a");
                let _ = fs::link(".././b", "/grindir/../a");
            }
            13 => {
                ProcessBuilder::new()
                    .spawn_fn(|| process::exit(0))
                    .unwrap()
                    .wait()
                    .unwrap();
            }
            14 => {
                ProcessBuilder::new()
                    .spawn_fn(|| {
                        let _ = process::fork().unwrap();
                        let _ = process::fork().unwrap();
                        process::exit(0);
                    })
                    .unwrap()
                    .wait()
                    .unwrap();
            }
            15 => {
                let _ = process::grow_break(6011).unwrap();
            }
            16 => {
                if process::current_break().addr() > break0 {
                    unsafe { process::shrink_break(process::current_break().addr() - break0) }
                        .unwrap();
                }
            }
            17 => {
                let mut child = ProcessBuilder::new()
                    .spawn_fn(|| {
                        let _ = File::options()
                            .read(true)
                            .write(true)
                            .create(true)
                            .open("a");
                        process::exit(0);
                    })
                    .unwrap();
                let _ = env::set_current_directory("../grindir/..");
                let _ = process::kill(child.id());
                child.wait().unwrap();
            }
            18 => {
                ProcessBuilder::new()
                    .spawn_fn(|| {
                        process::kill(process::id()).unwrap();
                        process::exit(0);
                    })
                    .unwrap()
                    .wait()
                    .unwrap();
            }
            19 => {
                let (mut rx, mut tx) = pipe::pipe().unwrap();
                let mut child = ProcessBuilder::new()
                    .spawn_fn(|| {
                        process::fork().unwrap();
                        process::fork().unwrap();
                        tx.write_all(b"x").unwrap();
                        let mut buf = [0; 1];
                        rx.read_exact(&mut buf).unwrap();
                        process::exit(0);
                    })
                    .unwrap();
                // close pipe before wait
                let _ = (rx, tx);
                child.wait().unwrap();
            }
            20 => {
                ProcessBuilder::new()
                    .spawn_fn(|| {
                        let _ = fs::remove_file("a");
                        let _ = fs::create_dir("a");
                        let _ = env::set_current_directory("a");
                        let _file = File::options()
                            .read(true)
                            .write(true)
                            .create(true)
                            .open("x");
                        let _ = fs::remove_file("x");
                        process::exit(0);
                    })
                    .unwrap()
                    .wait()
                    .unwrap();
            }
            21 => {
                let _ = fs::remove_file("c");
                // should always succeed. check that there are free i-nodes,
                // file descriptors, blocks.
                let mut fd1 = File::options()
                    .read(true)
                    .write(true)
                    .create(true)
                    .open("c")
                    .unwrap();
                fd1.write_all(b"x").unwrap();
                let st = fd1.metadata().unwrap();
                assert_eq!(st.size(), 1);
                assert!(st.ino() <= 200);
                drop(fd1);
                let _ = fs::remove_file("c");
            }
            22 => {
                let mut child_a = ProcessBuilder::new()
                    .stdout(Stdio::Pipe)
                    .spawn_fn(|| {
                        process::exec("grindir/../echo", &["echo", "hi"]).unwrap();
                        unreachable!();
                    })
                    .unwrap();
                let stdout_a = child_a.stdout.take().unwrap();

                let mut child_b = ProcessBuilder::new()
                    .stdin(Stdio::Fd(stdout_a.into()))
                    .stdout(Stdio::Pipe)
                    .spawn_fn(|| {
                        process::exec("/cat", &["cat"]).unwrap();
                        unreachable!();
                    })
                    .unwrap();

                let mut stdout_b = child_b.stdout.take().unwrap();
                let mut buf = [0; 3];
                stdout_b.read_exact(&mut buf).unwrap();
                assert_eq!(stdout_b.read(&mut [0]).unwrap(), 0);
                drop(stdout_b);
                assert_eq!(&buf, b"hi\n");
                assert!(child_a.wait().unwrap().success());
                assert!(child_b.wait().unwrap().success());
            }
            _ => unreachable!(),
        }
    }
}

fn iter() {
    let _ = fs::remove_file("a");
    let _ = fs::remove_file("b");

    let mut child1 = ProcessBuilder::new()
        .spawn_fn(|| {
            RAND_NEXT.fetch_xor(31, Ordering::Relaxed);
            go(b'A');
            process::exit(0);
        })
        .unwrap();

    let mut child2 = ProcessBuilder::new()
        .spawn_fn(|| {
            RAND_NEXT.fetch_xor(7177, Ordering::Relaxed);
            go(b'B');
            process::exit(0);
        })
        .unwrap();

    let (_pid, status) = process::wait_any().unwrap();
    if !status.success() {
        let _ = child1.kill();
        let _ = child2.kill();
    }
    process::wait_any().unwrap();
    process::exit(0);
}

fn main() {
    loop {
        ProcessBuilder::new()
            .spawn_fn(|| {
                iter();
                process::exit(0);
            })
            .unwrap()
            .wait()
            .unwrap();

        thread::sleep(Duration::from_secs(2));
        RAND_NEXT.fetch_add(1, Ordering::Relaxed);
    }
}
