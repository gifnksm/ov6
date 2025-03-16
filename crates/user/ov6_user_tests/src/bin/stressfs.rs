#![no_std]

use ov6_user_lib::{
    fs::File,
    io::{Read as _, Write as _},
    os_str::OsStr,
    process,
};
use ov6_user_tests::message;

fn main() {
    const N: u8 = 4;
    message!("starting");

    let mut path: [u8; 9] = *b"stressfs0";

    let mut idx = 0;
    for i in 0..N {
        let res = process::fork().unwrap();
        idx = i;
        if res.is_parent() {
            break;
        }
    }

    path[8] += idx;
    let path = OsStr::from_bytes(&path);

    let mut data = [b'a'; 512];

    message!("write {idx}");

    let mut file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .open(path)
        .unwrap();

    for _i in 0..20 {
        // message!("write {idx}-{_i}");
        file.write_all(&data).unwrap();
    }

    drop(file);

    message!("read {idx}");
    let mut file = File::open(path).unwrap();
    for _i in 0..20 {
        // message!("read {idx}-{_i}");
        file.read_exact(&mut data).unwrap();
    }
    drop(file);

    if idx != N - 1 {
        process::wait().unwrap();
    }
    process::exit(0);
}
