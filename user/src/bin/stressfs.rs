#![no_std]

extern crate alloc;

use core::ffi::CStr;

use ov6_user_lib::{
    fs::File,
    io::{Read as _, Write as _},
    process,
};
use user::{message, try_or_exit};

fn main() {
    message!("starting");

    let mut path: [u8; 10] = *b"stressfs0\0";

    let mut idx = 0;
    for i in 0..4 {
        let res = try_or_exit!(
            process::fork(),
            e => "fork failed: {e}"
        );
        idx = i;
        if res.is_parent() {
            break;
        }
    }

    path[8] += idx as u8;
    let path = CStr::from_bytes_until_nul(&path).unwrap();

    let mut data = [b'a'; 512];

    message!("write {idx}");

    let mut file = try_or_exit!(
        File::options().read(true).write(true).create(true).open(path),
        e => "open {} error: {e}", path.to_str().unwrap(),
    );

    for _i in 0..20 {
        // message!("write {idx}-{_i}");
        try_or_exit!(
            file.write_all(&data),
            e => "write {} error: {e}", path.to_str().unwrap(),
        );
    }

    drop(file);

    message!("read {idx}");
    let mut file = try_or_exit!(
        File::open(path),
        e => "open {} error: {e}", path.to_str().unwrap(),
    );
    for _i in 0..20 {
        // message!("read {idx}-{_i}");
        try_or_exit!(
            file.read_exact(&mut data),
            e => "read {} error: {e}", path.to_str().unwrap(),
        );
    }
    drop(file);

    try_or_exit!(
        process::wait(),
        e => "wait error: {e}",
    );
    process::exit(0);
}
