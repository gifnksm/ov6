use core::ffi::CStr;

use xv6_user_lib::{
    error::Error,
    fs::{self, File},
    io::{Read as _, Write as _},
    process,
};

use crate::expect;

const FILE_PATH: &CStr = c"truncfile";

/// test O_TRUNC.
pub fn test1() {
    let mut buf = [0; 6];

    let _ = fs::remove_file(FILE_PATH);

    let mut file1 = File::create(FILE_PATH).unwrap();
    file1.write(b"abcd").unwrap();
    drop(file1);

    let mut file2 = File::open(FILE_PATH).unwrap();
    expect!(file2.read(&mut buf), Ok(4));
    assert_eq!(buf[0..4], *b"abcd");

    let mut file1 = File::options()
        .write(true)
        .truncate(true)
        .open(FILE_PATH)
        .unwrap();

    let mut file3 = File::open(FILE_PATH).unwrap();
    expect!(file3.read(&mut buf), Ok(0));

    expect!(file2.read(&mut buf), Ok(0));

    file1.write_all(b"efghij").unwrap();

    file3.read_exact(&mut buf).unwrap();
    assert_eq!(buf[0..6], *b"efghij");

    expect!(file2.read(&mut buf), Ok(2));
    assert_eq!(buf[0..2], *b"ij");

    fs::remove_file(FILE_PATH).unwrap();
    drop(file1);
    drop(file2);
    drop(file3);
}

/// write to an open FD whose file has just been truncated.
/// this causes a write at an offset beyond the end of the file.
/// such writes fail on xv6 (unlike POSIX) but at least
/// they don't crash.
pub fn test2() {
    let mut file1 = File::create(FILE_PATH).unwrap();
    file1.write_all(b"abcd").unwrap();

    let _file2 = File::options()
        .write(true)
        .truncate(true)
        .open(FILE_PATH)
        .unwrap();

    expect!(file1.write(b"x"), Err(Error::Unknown));

    fs::remove_file(FILE_PATH).unwrap();
}

pub fn test3() {
    drop(File::create(FILE_PATH).unwrap());

    let mut buf = [0; 32];

    let child = process::fork_fn(|| {
        for _i in 0..100 {
            let mut file = File::options().write(true).open(FILE_PATH).unwrap();
            file.write_all(b"1234567890").unwrap();
            drop(file);
            let mut file = File::open(FILE_PATH).unwrap();
            file.read(&mut buf).unwrap();
            drop(file);
        }

        process::exit(0);
    })
    .unwrap();

    for _i in 0..150 {
        let mut file = File::create(FILE_PATH).unwrap();
        file.write_all(b"xxx").unwrap();
        drop(file);
    }

    let status = child.wait().unwrap();
    assert!(status.success());

    fs::remove_file(FILE_PATH).unwrap();
}
