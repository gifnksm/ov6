#![no_std]

use ov6_user_lib::{
    fs::File,
    io::{Read as _, Write as _},
    os_str::OsStr,
    process,
};
use user::{message, try_or_exit};

fn main() {
    message!("starting");

    let mut path: [u8; 9] = *b"stressfs0";

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

    path[8] += idx;
    let path = OsStr::from_bytes(&path);

    let mut data = [b'a'; 512];

    message!("write {idx}");

    let mut file = try_or_exit!(
        File::options().read(true).write(true).create(true).open(path),
        e => "open {} error: {e}", path.display(),
    );

    for _i in 0..20 {
        // message!("write {idx}-{_i}");
        try_or_exit!(
            file.write_all(&data),
            e => "write {} error: {e}", path.display(),
        );
    }

    drop(file);

    message!("read {idx}");
    let mut file = try_or_exit!(
        File::open(path),
        e => "open {} error: {e}", path.display(),
    );
    for _i in 0..20 {
        // message!("read {idx}-{_i}");
        try_or_exit!(
            file.read_exact(&mut data),
            e => "read {} error: {e}", path.display(),
        );
    }
    drop(file);

    try_or_exit!(
        process::wait(),
        e => "wait error: {e}",
    );
    process::exit(0);
}
