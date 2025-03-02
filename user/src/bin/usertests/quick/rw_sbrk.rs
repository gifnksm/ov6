use core::{ffi::CStr, slice};

use xv6_user_lib::{
    error::Error,
    fs::{self, File},
    io::{Read as _, Write as _},
    process,
};

use crate::{README_PATH, expect};

const FILE_PATH: &CStr = c"rw_sbrk";

/// See if the kernel refuses to read/write user memory that the
/// application doesn't have anymore, because it returned it.
pub fn test() {
    let a = process::grow_break(8192).unwrap();
    let _ = unsafe { process::shrink_break(8192) }.unwrap();

    let mut file = File::create(FILE_PATH).unwrap();
    unsafe {
        expect!(
            file.write(slice::from_raw_parts(a.add(4096), 1024)),
            Err(Error::Unknown),
        );
    }
    drop(file);
    fs::remove_file(FILE_PATH).unwrap();

    let mut file = File::open(README_PATH).unwrap();
    unsafe {
        expect!(
            file.read(slice::from_raw_parts_mut(a.add(4096), 10)),
            Err(Error::Unknown),
        );
    }
    drop(file);
}
