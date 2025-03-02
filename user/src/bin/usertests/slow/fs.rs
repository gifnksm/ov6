use core::{ffi::CStr, ptr, slice};

use xv6_fs_types::{FS_BLOCK_SIZE, MAX_FILE};
use xv6_user_lib::{
    error::Error,
    fs::{self, File},
    io::Write as _,
    process,
};

use crate::{BUF, expect};

/// directory that uses indirect blocks
pub fn big_dir() {
    const N: usize = 500;

    const FILE_PATH: &CStr = c"bd";

    let _ = fs::remove_file(FILE_PATH);

    let _ = File::create(FILE_PATH).unwrap();

    for i in 0..N {
        let mut name = [0; 4];
        name[0] = b'x';
        name[1] = b'0' + u8::try_from(i / 64).unwrap();
        name[2] = b'0' + u8::try_from(i % 64).unwrap();
        name[3] = b'\0';
        let path = CStr::from_bytes_with_nul(&name).unwrap();
        fs::link(FILE_PATH, path).unwrap();
    }

    fs::remove_file(FILE_PATH).unwrap();

    for i in 0..N {
        let mut name = [0; 4];
        name[0] = b'x';
        name[1] = b'0' + u8::try_from(i / 64).unwrap();
        name[2] = b'0' + u8::try_from(i % 64).unwrap();
        name[3] = b'\0';
        let path = CStr::from_bytes_with_nul(&name).unwrap();
        fs::remove_file(path).unwrap();
    }
}

/// concurrent writes to try to provoke deadlock in the virtio disk
/// driver.
pub fn many_writes() {
    let nchildren = 4;
    let howmany = 30; // increase to look for deadlock

    let buf = unsafe { (&raw mut BUF).as_mut() }.unwrap();

    for ci in 0..nchildren {
        process::fork_fn(|| {
            let mut name = [0; 3];
            name[0] = b'b';
            name[1] = b'a' + u8::try_from(ci).unwrap();
            name[2] = b'\0';
            let path = CStr::from_bytes_with_nul(&name).unwrap();
            let _ = fs::remove_file(path);

            for _ in 0..howmany {
                for _ in 0..ci + 1 {
                    let mut file = File::create(path).unwrap();
                    file.write_all(buf).unwrap();
                }
            }
            fs::remove_file(path).unwrap();
            process::exit(0);
        })
        .unwrap();
    }

    for _ in 0..nchildren {
        let (_, status) = process::wait().unwrap();
        assert!(status.success());
    }
}

/// regression test. does write() with an invalid buffer pointer cause
/// a block to be allocated for a file that is then not freed when the
/// file is deleted? if the kernel has this bug, it will panic: balloc:
/// out of blocks. assumed_free may need to be raised to be more than
/// the number of free blocks. this test takes a long time.
pub fn bad_write() {
    const FILE_PATH: &CStr = c"junk";

    let assumed_free = 600;

    let _ = fs::remove_file(FILE_PATH);
    for _ in 0..assumed_free {
        let mut file = File::create(FILE_PATH).unwrap();
        unsafe {
            let buf = slice::from_raw_parts(ptr::with_exposed_provenance(0xff_ffff_ffff), 1);
            expect!(file.write(buf), Err(Error::Unknown));
            drop(file);
            fs::remove_file(FILE_PATH).unwrap();
        }
    }

    let mut file = File::create(FILE_PATH).unwrap();
    file.write_all(b"x").unwrap();
    drop(file);

    fs::remove_file(FILE_PATH).unwrap();
}

/// can the kernel tolerate running out of disk space?
pub fn disk_full() {
    const DIR_PATH: &CStr = c"diskfulldir";
    let _ = fs::remove_file(DIR_PATH);

    'outer: for fc in b'0'..0o177 {
        let name = [b'b', b'i', b'g', fc, b'\0'];
        let path = CStr::from_bytes_with_nul(&name).unwrap();
        let Ok(mut file) = File::create(path) else {
            break;
        };
        for _ in 0..MAX_FILE {
            let buf = [0; FS_BLOCK_SIZE];
            if file.write_all(&buf).is_err() {
                break 'outer;
            }
        }
    }

    // now that there are no free blocks, test that dirlink()
    // memory fails (doesn't panic) if it can't extend
    // directory content. one of these file creations
    // is expected to fail.
    let nzz = 128;
    for i in 0..nzz {
        let name = [
            b'z',
            b'z',
            b'0' + u8::try_from(i / 32).unwrap(),
            b'0' + u8::try_from(i % 32).unwrap(),
            b'\0',
        ];
        let path = CStr::from_bytes_with_nul(&name).unwrap();
        let _ = fs::remove_file(path);
        let Ok(_) = File::create(path) else {
            break;
        };
    }

    // this mkdir() is expected to fail.
    expect!(fs::create_dir(DIR_PATH), Err(Error::Unknown));
    let _ = fs::remove_file(DIR_PATH);

    for i in 0..nzz {
        let name = [
            b'z',
            b'z',
            b'0' + u8::try_from(i / 32).unwrap(),
            b'0' + u8::try_from(i % 32).unwrap(),
            b'\0',
        ];
        let path = CStr::from_bytes_with_nul(&name).unwrap();
        let _ = fs::remove_file(path);
    }

    for fc in b'0'..0o177 {
        let name = [b'b', b'i', b'g', fc, b'\0'];
        let path = CStr::from_bytes_with_nul(&name).unwrap();
        let _ = fs::remove_file(path);
    }
}

pub fn out_of_inodes() {
    let nzz = 32 * 32;
    for i in 0..nzz {
        let name = [
            b'z',
            b'z',
            b'0' + u8::try_from(i / 32).unwrap(),
            b'0' + u8::try_from(i % 32).unwrap(),
            b'\0',
        ];
        let path = CStr::from_bytes_with_nul(&name).unwrap();
        let _ = fs::remove_file(path);
        let Ok(_file) = File::create(path) else {
            break;
        };
    }

    let nzz = 32 * 32;
    for i in 0..nzz {
        let name = [
            b'z',
            b'z',
            b'0' + u8::try_from(i / 32).unwrap(),
            b'0' + u8::try_from(i % 32).unwrap(),
            b'\0',
        ];
        let path = CStr::from_bytes_with_nul(&name).unwrap();
        let _ = fs::remove_file(path);
    }
}
