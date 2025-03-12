#![no_std]

use core::{
    ptr,
    sync::atomic::{AtomicU64, Ordering},
};

use ov6_user_lib::{
    env,
    fs::{self, File},
    io::{Read as _, STDIN_FD, STDOUT_FD, Write as _},
    os::{fd::AsRawFd as _, ov6::syscall},
    pipe, print, process, thread,
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
    let x = 16807 * lo - 2836 * hi;

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
fn go(name: char) {
    let mut buf = [0; 999];
    let break0 = process::current_break().addr();

    let _ = fs::create_dir("grindir");
    env::set_current_directory("grindir").unwrap();
    env::set_current_directory("/").unwrap();

    let mut file = None;

    for iters in 1.. {
        if iters % 500 == 0 {
            print!("{name}");
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
                process::fork_fn(|| process::exit(0))
                    .unwrap()
                    .wait()
                    .unwrap();
            }
            14 => {
                process::fork_fn(|| {
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
                let child = process::fork_fn(|| {
                    let _ = File::options()
                        .read(true)
                        .write(true)
                        .create(true)
                        .open("a");
                    process::exit(0);
                })
                .unwrap();
                let _ = env::set_current_directory("../grindir/..");
                let _ = process::kill(child.pid());
                child.wait().unwrap();
            }
            18 => {
                process::fork_fn(|| {
                    process::kill(process::id()).unwrap();
                    process::exit(0);
                })
                .unwrap()
                .wait()
                .unwrap();
            }
            19 => {
                let (mut rx, mut tx) = pipe::pipe().unwrap();
                let child = process::fork_fn(|| {
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
                process::fork_fn(|| {
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
                let (arx, atx) = pipe::pipe().unwrap();
                let (mut brx, btx) = pipe::pipe().unwrap();

                if process::fork().unwrap().is_child() {
                    drop(brx);
                    drop(btx);
                    drop(arx);
                    unsafe { syscall::close(STDOUT_FD) }.unwrap();
                    let atx2 = atx.try_clone().unwrap();
                    assert_eq!(atx2.as_raw_fd(), STDOUT_FD);
                    drop(atx);
                    let args = [c"echo".as_ptr(), c"hi".as_ptr(), ptr::null()];
                    process::exec(c"grindir/../echo", &args).unwrap();
                    unreachable!();
                }

                if process::fork().unwrap().is_child() {
                    drop(atx);
                    drop(brx);
                    unsafe { syscall::close(STDIN_FD) }.unwrap();
                    let arx2 = arx.try_clone().unwrap();
                    assert_eq!(arx2.as_raw_fd(), STDIN_FD);
                    drop(arx);
                    unsafe { syscall::close(STDOUT_FD) }.unwrap();
                    let btx2 = btx.try_clone().unwrap();
                    assert_eq!(btx2.as_raw_fd(), STDOUT_FD);
                    drop(btx);
                    let args = [c"cat".as_ptr(), ptr::null()];
                    process::exec(c"/cat", &args).unwrap();
                    unreachable!();
                }

                drop(arx);
                drop(atx);
                drop(btx);
                let mut buf = [0; 4];
                brx.read_exact(&mut buf[0..1]).unwrap();
                brx.read_exact(&mut buf[1..2]).unwrap();
                brx.read_exact(&mut buf[2..3]).unwrap();
                drop(brx);
                let (_, st1) = process::wait().unwrap();
                let (_, st2) = process::wait().unwrap();
                assert!(st1.success());
                assert!(st2.success());
                assert_eq!(&buf, b"hi\n\0");
            }
            _ => unreachable!(),
        }
    }
}

fn iter() {
    let _ = fs::remove_file("a");
    let _ = fs::remove_file("b");

    let child1 = process::fork_fn(|| {
        RAND_NEXT.fetch_xor(31, Ordering::Relaxed);
        go('A');
        process::exit(0);
    })
    .unwrap();

    let child2 = process::fork_fn(|| {
        RAND_NEXT.fetch_xor(7177, Ordering::Relaxed);
        go('B');
        process::exit(0);
    })
    .unwrap();

    let (_pid, status) = process::wait().unwrap();
    if !status.success() {
        let _ = process::kill(child1.pid());
        let _ = process::kill(child2.pid());
    }
    process::wait().unwrap();
    process::exit(0);
}

fn main() {
    loop {
        process::fork_fn(|| {
            iter();
            process::exit(0);
        })
        .unwrap()
        .wait()
        .unwrap();

        thread::sleep(20);
        RAND_NEXT.fetch_add(1, Ordering::Relaxed);
    }
}
