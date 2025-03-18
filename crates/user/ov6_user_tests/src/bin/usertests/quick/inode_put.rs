use core::time::Duration;

use ov6_user_lib::{
    env,
    error::Ov6Error,
    fs::{self, File},
    process, thread,
};

use crate::{ROOT_DIR_PATH, expect};

const IPUTDIR_PATH: &str = "iputdir";
const OIDIR_PATH: &str = "oidir";

/// does the error path in `open()` for attempt to write a
/// directory call `Inode::put()` in a transaction?
/// needs a hacked kernel that pauses just after the `namei()`
/// call in `sys_open()`:
///
/// ```c
/// if((ip = namei(path)) == 0)
///   return -1;
/// {
///   int i;
///   for(i = 0; i < 10000; i++)
///     yield();
/// }
/// ```
pub fn inode_put_open() {
    fs::create_dir(OIDIR_PATH).unwrap();

    let child = process::fork_fn(|| {
        expect!(
            File::options().read(true).write(true).open(OIDIR_PATH),
            Err(Ov6Error::IsADirectory),
        );
        process::exit(0);
    })
    .unwrap();

    thread::sleep(Duration::from_millis(100));
    fs::remove_file(OIDIR_PATH).unwrap();

    let status = child.wait().unwrap();
    assert!(status.success());
}

/// does `exit()` call `Inode::put(p->cwd)` in a transaction?
pub fn inode_put_exit() {
    let status = process::fork_fn(|| {
        fs::create_dir(IPUTDIR_PATH).unwrap();
        env::set_current_directory(IPUTDIR_PATH).unwrap();
        fs::remove_file("../iputdir").unwrap();
        process::exit(0);
    })
    .unwrap()
    .wait()
    .unwrap();
    assert!(status.success());
}

/// does `chdir()` call `Inode::put(p->cwd)` in a transaction?
pub fn inode_put_chdir() {
    fs::create_dir(IPUTDIR_PATH).unwrap();
    env::set_current_directory(IPUTDIR_PATH).unwrap();
    fs::remove_file("../iputdir").unwrap();
    env::set_current_directory(ROOT_DIR_PATH).unwrap();
}
