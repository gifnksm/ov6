#![no_std]

use xv6_user_lib::{eprintln, process};

const N: usize = 1000;

fn forktest() {
    eprintln!("fork test");

    let mut n = 0;

    for i in 0..N {
        n = i;
        let Ok(pid) = process::fork() else {
            break;
        };

        if pid == 0 {
            // child process
            process::exit(0);
        }
    }

    if n == N {
        panic!("fork claimed to work N times!");
    }

    for _ in 0..n {
        let Ok(status) = process::wait() else {
            panic!("wait stopped early");
        };

        if !status.success() {
            panic!("child failed");
        }
    }

    if process::wait().is_ok() {
        panic!("wait got too many");
    }

    eprintln!("fork test OK (n={n})");
}

fn main() {
    forktest();
    process::exit(0);
}
