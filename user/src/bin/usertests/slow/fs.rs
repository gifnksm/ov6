use core::{ptr, slice};

use ov6_fs_types::{FS_BLOCK_SIZE, MAX_FILE};
use ov6_user_lib::{
    error::Ov6Error,
    fs::{self, File},
    io::Write as _,
    os_str::OsStr,
    process,
};

use crate::{BUF, expect};

/// directory that uses indirect blocks
pub fn big_dir() {
    const N: usize = 500;

    const FILE_PATH: &str = "bd";

    let _ = fs::remove_file(FILE_PATH);

    let _ = File::create(FILE_PATH).unwrap();

    for i in 0..N {
        let name = [
            b'x',
            b'0' + u8::try_from(i / 64).unwrap(),
            b'0' + u8::try_from(i % 64).unwrap(),
        ];
        let path = OsStr::from_bytes(&name);
        fs::link(FILE_PATH, path).unwrap();
    }

    fs::remove_file(FILE_PATH).unwrap();

    for i in 0..N {
        let name = [
            b'x',
            b'0' + u8::try_from(i / 64).unwrap(),
            b'0' + u8::try_from(i % 64).unwrap(),
        ];
        let path = OsStr::from_bytes(&name);
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
            let name = [b'b', b'a' + u8::try_from(ci).unwrap()];
            let path = OsStr::from_bytes(&name);

            let _ = fs::remove_file(path);

            for _ in 0..howmany {
                for _ in 0..=ci {
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

/// regression test. does `write()` with an invalid buffer pointer cause
/// a block to be allocated for a file that is then not freed when the
/// file is deleted? if the kernel has this bug, it will panic: balloc:
/// out of blocks. `assumed_free` may need to be raised to be more than
/// the number of free blocks. this test takes a long time.
pub fn bad_write() {
    const FILE_PATH: &str = "junk";

    let assumed_free = 600;

    let _ = fs::remove_file(FILE_PATH);
    for _ in 0..assumed_free {
        let mut file = File::create(FILE_PATH).unwrap();
        unsafe {
            let buf = slice::from_raw_parts(ptr::with_exposed_provenance(0xff_ffff_ffff), 1);
            expect!(file.write(buf), Err(Ov6Error::BadAddress));
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
    const DIR_PATH: &str = "diskfulldir";
    let _ = fs::remove_file(DIR_PATH);

    'outer: for fc in b'0'..0o177 {
        let name = [b'b', b'i', b'g', fc];
        let path = OsStr::from_bytes(&name);
        let Ok(mut file) = File::create(path) else {
            break;
        };
        for _ in 0..MAX_FILE {
            let buf = [0; FS_BLOCK_SIZE];
            if let Err(e) = file.write_all(&buf) {
                expect!(e, Ov6Error::StorageFull);
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
        ];
        let path = OsStr::from_bytes(&name);
        let _ = fs::remove_file(path);
        match File::create(path) {
            Ok(_) => continue,
            Err(Ov6Error::StorageFull) => break,
            Err(e) => panic!("unexpected error: {e:?}"),
        }
    }

    // this mkdir() is expected to fail.
    expect!(fs::create_dir(DIR_PATH), Err(Ov6Error::StorageFull));
    let _ = fs::remove_file(DIR_PATH);

    for i in 0..nzz {
        let name = [
            b'z',
            b'z',
            b'0' + u8::try_from(i / 32).unwrap(),
            b'0' + u8::try_from(i % 32).unwrap(),
        ];
        let path = OsStr::from_bytes(&name);
        let _ = fs::remove_file(path);
    }

    for fc in b'0'..0o177 {
        let name = [b'b', b'i', b'g', fc];
        let path = OsStr::from_bytes(&name);
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
        ];
        let path = OsStr::from_bytes(&name);
        let _ = fs::remove_file(path);
        match File::create(path) {
            Ok(_) => continue,
            Err(Ov6Error::StorageFull) => break,
            Err(e) => panic!("unexpected error: {e:?}"),
        }
    }

    let nzz = 32 * 32;
    for i in 0..nzz {
        let name = [
            b'z',
            b'z',
            b'0' + u8::try_from(i / 32).unwrap(),
            b'0' + u8::try_from(i % 32).unwrap(),
        ];
        let path = OsStr::from_bytes(&name);
        let _ = fs::remove_file(path);
    }
}
