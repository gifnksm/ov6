#![no_std]

use user::{message, try_or_panic};
use xv6_user_lib::process;

const N: usize = 1000;

fn forktest() {
    message!("start");

    let mut n = 0;

    for i in 0..N {
        n = i;
        let Ok(res) = process::fork() else {
            break;
        };
        if res.is_child() {
            process::exit(0);
        }
    }

    assert_ne!(n, N, "fork claimed to work N times!");

    for _ in 0..n {
        let (_pid, status) = try_or_panic!(
            process::wait(),
            e => "wait stopped early: {e}"
        );
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
