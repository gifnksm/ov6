#![no_std]

use ov6_user_lib::process;
use tests::message;

const N: usize = 1000;

fn forktest() {
    message!("start");

    let mut n = 0;

    for i in 0..N {
        n = i;
        if process::fork_fn(|| process::exit(0)).is_err() {
            break;
        }
    }

    assert_ne!(n, N, "fork claimed to work N times!");

    for _ in 0..n {
        let (_pid, status) = process::wait().unwrap();
        assert!(status.success(), "child failed");
    }

    assert!(process::wait().is_err(), "wait got too manye");

    message!("{n} processes forked");
    message!("OK");
}

fn main() {
    forktest();
    process::exit(0);
}
