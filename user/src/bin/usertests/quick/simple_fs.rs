use core::{ffi::CStr, ptr};

use xv6_fs_types::{FS_BLOCK_SIZE, MAX_FILE};
use xv6_user_lib::{
    env,
    error::Error,
    fs::{self, File},
    io::{Read, STDOUT_FD, Write as _},
    os::{fd::AsRawFd, xv6::syscall},
    process,
};

use crate::{BUF, ECHO_PATH, expect};

const NOT_EXIST_PATH: &CStr = c"doesnotexist";
const SMALL_PATH: &CStr = c"small";
const BIG_PATH: &CStr = c"big";

pub fn open_test() {
    let file = File::open(ECHO_PATH).unwrap();
    drop(file);
    expect!(File::open(NOT_EXIST_PATH), Err(Error::Unknown));
}

pub fn write_test() {
    const N: usize = 100;
    const SIZE: usize = 10;

    let buf = unsafe { (&raw mut BUF).as_mut() }.unwrap();

    let mut file = File::options()
        .create(true)
        .read(true)
        .write(true)
        .open(SMALL_PATH)
        .unwrap();
    for _i in 0..N {
        file.write_all(b"aaaaaaaaaa").unwrap();
        file.write_all(b"bbbbbbbbbb").unwrap();
    }
    drop(file);

    let mut file = File::open(SMALL_PATH).unwrap();
    file.read_exact(&mut buf[..N * SIZE]).unwrap();
    drop(file);

    fs::remove_file(SMALL_PATH).unwrap();
}

pub fn write_big_test() {
    let buf = unsafe { (&raw mut BUF).as_mut() }.unwrap();

    let mut file = File::options()
        .create(true)
        .read(true)
        .write(true)
        .open(BIG_PATH)
        .unwrap();

    for i in 0..MAX_FILE {
        buf[0..size_of::<usize>()].copy_from_slice(&i.to_ne_bytes());
        file.write_all(&buf[..FS_BLOCK_SIZE]).unwrap();
    }
    drop(file);

    let mut file = File::open(BIG_PATH).unwrap();
    let mut n = 0;
    loop {
        let i = file.read(&mut buf[..FS_BLOCK_SIZE]).unwrap();
        if i == 0 {
            assert_eq!(n, MAX_FILE, "read only {n} blocks from big");
            break;
        }
        assert_eq!(i, FS_BLOCK_SIZE);

        assert_eq!(
            usize::from_ne_bytes(buf[0..size_of::<usize>()].try_into().unwrap()),
            n
        );
        n += 1;
    }
    drop(file);
    fs::remove_file(BIG_PATH).unwrap();
}

/// many creates, followed by unlink test
pub fn create_test() {
    const N: usize = 52;

    let mut name = *b"a_\0";

    for i in 0..N {
        name[1] = b'0' + u8::try_from(i).unwrap();
        let path = CStr::from_bytes_with_nul(&name).unwrap();
        let _file = File::options()
            .create(true)
            .read(true)
            .write(true)
            .open(path)
            .unwrap();
    }

    for i in 0..N {
        name[1] = b'0' + u8::try_from(i).unwrap();
        let path = CStr::from_bytes_with_nul(&name).unwrap();
        fs::remove_file(path).unwrap();
    }
}

pub fn dir_test() {
    const DIR_PATH: &CStr = c"dir0";
    fs::create_dir(DIR_PATH).unwrap();
    env::set_current_directory(DIR_PATH).unwrap();
    env::set_current_directory(c"..").unwrap();
    fs::remove_file(DIR_PATH).unwrap();
}

pub fn exec_test() {
    const ECHO_OK_PATH: &CStr = c"echo-ok";

    let echo_argv = [c"echo".as_ptr(), c"OK".as_ptr(), ptr::null()];
    let _ = fs::remove_file(ECHO_OK_PATH);

    let status = process::fork_fn(|| {
        unsafe { syscall::close(STDOUT_FD) }.unwrap();
        let file = File::create(ECHO_OK_PATH).unwrap();
        assert_eq!(file.as_raw_fd(), STDOUT_FD);

        process::exec(ECHO_PATH, &echo_argv).unwrap();
        unreachable!();
    })
    .unwrap()
    .wait()
    .unwrap();
    assert!(status.success());

    let mut file = File::open(ECHO_OK_PATH).unwrap();
    let mut buf = [0; 2];
    file.read_exact(&mut buf[0..2]).unwrap();
    fs::remove_file(ECHO_OK_PATH).unwrap();
    assert_eq!(buf, *b"OK");
}
