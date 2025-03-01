#![no_std]

use core::{
    ptr,
    sync::atomic::{AtomicU64, Ordering},
};

use user::try_or_exit;
use xv6_user_lib::{
    env,
    fs::{self, File, OpenFlags},
    io::{Read as _, STDIN_FD, STDOUT_FD, Write as _},
    os::{fd::AsRawFd, xv6::syscall},
    pipe, print,
    process::{self, ForkResult},
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
    let x = (*ctx % 0x7ffffffe) + 1;
    let hi = x / 127773;
    let lo = x % 127773;
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

fn go(name: char) {
    let mut buf = [0; 999];
    let break0 = process::current_break().addr();

    let _ = fs::create_dir(c"grindir");
    env::set_current_directory(c"grindir").unwrap();
    env::set_current_directory(c"/").unwrap();

    let mut file = None;

    for iters in 1.. {
        if iters % 500 == 0 {
            print!("{name}");
        }

        match rand() % 23 {
            0 => {}
            1 => {
                let _ = File::open(c"grindir/../a", OpenFlags::CREATE | OpenFlags::READ_WRITE);
            }
            2 => {
                let _ = File::open(
                    c"grindir/../grindir/../b",
                    OpenFlags::CREATE | OpenFlags::READ_WRITE,
                );
            }
            3 => {
                let _ = fs::remove_file(c"grindir/../a");
            }
            4 => {
                env::set_current_directory(c"grindir").unwrap();
                let _ = fs::remove_file(c"../b");
                env::set_current_directory(c"/").unwrap();
            }
            5 => {
                let _ = file.take();
                file = File::open(c"/grindir/../a", OpenFlags::CREATE | OpenFlags::READ_WRITE).ok();
            }
            6 => {
                let _ = file.take();
                file = File::open(
                    c"/./grindir/./../b",
                    OpenFlags::CREATE | OpenFlags::READ_WRITE,
                )
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
                let _ = fs::create_dir(c"grindir/../a");
                let _ = File::open(c"a/../a/./a", OpenFlags::CREATE | OpenFlags::READ_WRITE);
                let _ = fs::remove_file(c"a/a");
            }
            10 => {
                let _ = fs::create_dir(c"/../b");
                let _ = File::open(c"grindir/../b/b", OpenFlags::CREATE | OpenFlags::READ_WRITE);
                let _ = fs::remove_file(c"b/b");
            }
            11 => {
                let _ = fs::remove_file(c"b");
                let _ = fs::link(c"../grindir/./../a", c"../b");
            }
            12 => {
                let _ = fs::remove_file(c"../grindir/../a");
                let _ = fs::link(c".././b", c"/grindir/../a");
            }
            13 => match process::fork().unwrap() {
                ForkResult::Child => {
                    process::exit(0);
                }
                ForkResult::Parent { child: _ } => {
                    process::wait().unwrap();
                }
            },
            14 => match process::fork().unwrap() {
                ForkResult::Child => {
                    let _ = process::fork().unwrap();
                    let _ = process::fork().unwrap();
                    process::exit(0);
                }
                ForkResult::Parent { child: _ } => {
                    process::wait().unwrap();
                }
            },
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
                match process::fork().unwrap() {
                    ForkResult::Child => {
                        let _ = File::open(c"a", OpenFlags::CREATE | OpenFlags::READ_WRITE);
                        process::exit(0);
                    }
                    ForkResult::Parent { child: pid } => {
                        env::set_current_directory(c"../grindir/..").unwrap();
                        let _ = process::kill(pid);
                        process::wait().unwrap();
                    }
                };
            }
            18 => match process::fork().unwrap() {
                ForkResult::Child => {
                    process::kill(process::id()).unwrap();
                    process::exit(0);
                }
                ForkResult::Parent { .. } => {
                    process::wait().unwrap();
                }
            },
            19 => {
                let (mut rx, mut tx) = pipe::pipe().unwrap();
                match process::fork().unwrap() {
                    ForkResult::Child => {
                        process::fork().unwrap();
                        process::fork().unwrap();
                        tx.write_all(b"x").unwrap();
                        let mut buf = [0; 1];
                        rx.read_exact(&mut buf).unwrap();
                        process::exit(0);
                    }
                    ForkResult::Parent { .. } => {
                        let _ = (rx, tx);
                        process::wait().unwrap();
                    }
                }
            }
            20 => match process::fork().unwrap() {
                ForkResult::Child => {
                    let _ = fs::remove_file(c"a");
                    let _ = fs::create_dir(c"a");
                    let _ = env::set_current_directory(c"a");
                    let _file = File::open(c"x", OpenFlags::CREATE | OpenFlags::READ_WRITE);
                    let _ = fs::remove_file(c"x");
                    process::exit(0);
                }
                ForkResult::Parent { child: _ } => {
                    process::wait().unwrap();
                }
            },
            21 => {
                let _ = fs::remove_file(c"c");
                // should always succeed. check that there are free i-nodes,
                // file descriptors, blocks.
                let mut fd1 = File::open(c"c", OpenFlags::CREATE | OpenFlags::READ_WRITE).unwrap();
                fd1.write_all(b"x").unwrap();
                let st = fd1.metadata().unwrap();
                assert_eq!(st.size(), 1);
                assert!(st.ino() <= 200);
                drop(fd1);
                let _ = fs::remove_file(c"c");
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
    let _ = fs::remove_file(c"a");
    let _ = fs::remove_file(c"b");

    let pid1 = match process::fork().unwrap() {
        ForkResult::Child => {
            RAND_NEXT.fetch_xor(31, Ordering::Relaxed);
            go('A');
            process::exit(0);
        }
        ForkResult::Parent { child } => child,
    };

    let pid2 = match process::fork().unwrap() {
        ForkResult::Child => {
            RAND_NEXT.fetch_xor(7177, Ordering::Relaxed);
            go('B');
            process::exit(0);
        }
        ForkResult::Parent { child } => child,
    };

    let (_pid, status) = try_or_exit!(
        process::wait(),
        e => "wait failed: {e}",
    );
    if !status.success() {
        let _ = process::kill(pid1);
        let _ = process::kill(pid2);
    }
    let (_pid, _status) = try_or_exit!(
        process::wait(),
        e => "wait failed: {e}",
    );

    process::exit(0);
}

fn main() {
    loop {
        match process::fork().unwrap() {
            ForkResult::Child => {
                iter();
                process::exit(0);
            }
            ForkResult::Parent { .. } => {
                try_or_exit!(
                    process::wait(),
                    e => "wait failed: {e}",
                );
            }
        }

        try_or_exit!(
            thread::sleep(20),
            e => "sleep failed: {e}",
        );
        RAND_NEXT.fetch_add(1, Ordering::Relaxed);
    }
}
