use alloc::vec;

use ov6_fs_types::{FS_BLOCK_SIZE, MAX_FILE};
use ov6_user_lib::{
    env,
    error::Ov6Error,
    fs::{self, File},
    io::{Read as _, STDOUT_FD, Write as _},
    os::{
        fd::{AsRawFd as _, RawFd},
        ov6::syscall,
    },
    os_str::OsStr,
    process::{self, ProcessBuilder},
};
use ov6_user_tests::expect;

use crate::{BUF, ECHO_PATH};

const NOT_EXIST_PATH: &str = "doesnotexist";
const SMALL_PATH: &str = "small";
const BIG_PATH: &str = "big";

pub fn open_test() {
    let file = File::open(ECHO_PATH).unwrap();
    drop(file);
    expect!(File::open(NOT_EXIST_PATH), Err(Ov6Error::FsEntryNotFound));
}

pub fn too_many_open_files() {
    let mut files = vec![];
    loop {
        match File::open(ECHO_PATH) {
            Ok(file) => files.push(file),
            Err(e) => {
                expect!(e, Ov6Error::TooManyOpenFiles);
                break;
            }
        }
    }
    drop(files);
}

pub fn too_many_open_files_in_system() {
    let mut files = vec![];
    loop {
        let Some(mut child) = process::fork().unwrap().into_parent() else {
            files.clear();
            loop {
                match File::open(ECHO_PATH) {
                    Ok(file) => files.push(file),
                    Err(Ov6Error::TooManyOpenFiles) => break,
                    Err(Ov6Error::TooManyOpenFilesSystem) => process::exit(0),
                    Err(e) => panic!("unexpected error: {e:?}"),
                }
            }
            continue;
        };
        assert!(child.wait().unwrap().success());
        break;
    }
    drop(files);
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

    let mut name = *b"a_";

    for i in 0..N {
        name[1] = b'0' + u8::try_from(i).unwrap();
        let path = OsStr::from_bytes(&name);
        let _file = File::options()
            .create(true)
            .read(true)
            .write(true)
            .open(path)
            .unwrap();
    }

    for i in 0..N {
        name[1] = b'0' + u8::try_from(i).unwrap();
        let path = OsStr::from_bytes(&name);
        fs::remove_file(path).unwrap();
    }
}

pub fn dir_test() {
    const DIR_PATH: &str = "dir0";
    fs::create_dir(DIR_PATH).unwrap();
    env::set_current_directory(DIR_PATH).unwrap();
    env::set_current_directory("..").unwrap();
    fs::remove_file(DIR_PATH).unwrap();
}

pub fn exec_test() {
    const ECHO_OK_PATH: &str = "echo-ok";

    let echo_argv = ["echo", "OK"];
    let _ = fs::remove_file(ECHO_OK_PATH);

    let status = ProcessBuilder::new()
        .spawn_fn(|| {
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

pub fn bad_fd() {
    for fd in [4, 15, 16, 1024, usize::MAX] {
        let fd = RawFd::new(fd);
        expect!(syscall::dup(fd), Err(Ov6Error::BadFileDescriptor));
        expect!(syscall::read(fd, &mut []), Err(Ov6Error::BadFileDescriptor));
        expect!(syscall::write(fd, &[]), Err(Ov6Error::BadFileDescriptor));
        expect!(
            unsafe { syscall::close(fd) },
            Err(Ov6Error::BadFileDescriptor)
        );
        expect!(syscall::fstat(fd), Err(Ov6Error::BadFileDescriptor));
    }
}
