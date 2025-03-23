#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use core::str::FromStr as _;

use ov6_user_lib::{
    env,
    os::ov6::syscall::{self, SyscallCode},
    process,
};
use ov6_utilities::{exit, message_err, usage_and_exit};

fn main() {
    let mut args = env::args_os();
    if args.len() < 2 {
        usage_and_exit!("<mask> [command...]");
    }

    let mut mask = 0;
    let mask_str = args.next().unwrap();
    let mask_str = mask_str
        .to_str()
        .unwrap_or_else(|| exit!("invalid mask '{}'", mask_str.display()));

    for part in mask_str.split(',') {
        if part.is_empty() {
            continue;
        }
        if let Some(hex) = part.strip_prefix("0x").or_else(|| part.strip_prefix("0X")) {
            mask |=
                u64::from_str_radix(hex, 16).unwrap_or_else(|_| exit!("invalid mask '{}'", part));
            continue;
        }
        if part.starts_with(|c: char| c.is_numeric()) {
            mask |= u64::from_str(part).unwrap_or_else(|_| exit!("invalid mask '{}'", part));
            continue;
        }
        if part == "all" {
            mask = u64::MAX;
            break;
        }
        if part == "none" {
            mask = 0;
            break;
        }
        let Ok(syscall) =
            SyscallCode::from_str(part).map_err(|_e| exit!("invalid mask '{}'", part));
        mask |= 1 << syscall as usize;
    }

    let args = args.collect::<Vec<_>>();

    syscall::trace(mask);

    let arg0 = args.first().unwrap();
    let Err(e) = process::exec(arg0, &args);
    message_err!(e, "failed to exec '{}'", arg0.display());
}
