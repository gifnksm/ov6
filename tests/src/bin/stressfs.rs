#![no_std]

use ov6_user_lib::{
    fs::File,
    io::{Read as _, Write as _},
    os_str::OsStr,
    process,
};
use tests::message;

fn main() {
    message!("starting");

    let mut path: [u8; 9] = *b"stressfs0";

    let mut idx = 0;
    for i in 0..4 {
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

    process::wait().unwrap();
    process::exit(0);
}
