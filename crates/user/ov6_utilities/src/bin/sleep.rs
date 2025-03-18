#![no_std]

use core::time::Duration;

use ov6_user_lib::{env, thread};
use ov6_utilities::usage_and_exit;

fn main() {
    let mut args = env::args();

    if args.len() != 1 {
        usage_and_exit!("seconds");
    }

    let sec = args.next().unwrap().parse().unwrap();
    let dur = Duration::from_secs(sec);

    thread::sleep(dur);
}
