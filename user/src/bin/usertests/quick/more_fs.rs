use core::ffi::CStr;

use ov6_fs_types::FS_BLOCK_SIZE;
use ov6_kernel_params::{MAX_OP_BLOCKS, NINODE};
use ov6_user_lib::{
    env,
    error::Ov6Error,
    fs::{self, File},
    io::{Read as _, Write as _},
    process,
};

use crate::{BUF, README_PATH, expect};

/// two processes write to the same file descriptor
/// is the offset shared? does inode locking work?
pub fn shared_fd() {
    const FILE_PATH: &CStr = c"sharedfd";
    const N: usize = 1000;
    const SIZE: usize = 10;

    let mut buf = [0; SIZE];

    let _ = fs::remove_file(FILE_PATH);
    let mut file = File::create(FILE_PATH).unwrap();
    let res = process::fork().unwrap();
    buf.fill(if res.is_child() { b'c' } else { b'p' });
    for _ in 0..N {
        file.write_all(&buf).unwrap();
    }
    if res.is_child() {
        process::exit(0);
    }
    let (_, status) = process::wait().unwrap();
    assert!(status.success());
    drop(file);

    let mut file = File::open(FILE_PATH).unwrap();
    let mut buf = [0; SIZE];
    let mut nc = 0;
    let mut np = 0;

    loop {
        let n = file.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        assert_eq!(n, SIZE);
        for &c in &buf {
            if c == b'c' {
                nc += 1;
            } else if c == b'p' {
                np += 1;
            } else {
                panic!("unexpected char");
            }
        }
    }
    drop(file);
    fs::remove_file(FILE_PATH).unwrap();
    assert_eq!(nc, N * SIZE);
    assert_eq!(np, N * SIZE);
}

pub fn four_files() {
    const N: usize = 12;
    const SIZE: usize = 500;

    let buf = unsafe { (&raw mut BUF).as_mut() }.unwrap();
    let buf = &mut buf[..SIZE];

    let names = [c"f0", c"f1", c"f2", c"f3"];
    let bytes = [b'0', b'1', b'2', b'3'];
    for (&name, &b) in names.iter().zip(&bytes) {
        let _ = fs::remove_file(name);
        process::fork_fn(|| {
            let mut file = File::create(name).unwrap();
            buf.fill(b);
            for _ in 0..N {
                file.write_all(buf).unwrap();
            }
            process::exit(0);
        })
        .unwrap();
    }

    for _ in 0..names.len() {
        let (_, status) = process::wait().unwrap();
        assert!(status.success());
    }

    for (&name, &b) in names.iter().zip(&bytes) {
        let mut file = File::open(name).unwrap();
        let mut total = 0;
        loop {
            let n = file.read(buf).unwrap();
            if n == 0 {
                break;
            }
            for &c in buf.iter().take(n) {
                assert_eq!(c, b);
            }
            total += n;
        }
        drop(file);
        assert_eq!(total, N * SIZE);
        fs::remove_file(name).unwrap();
    }
}

/// four processes create and delete different files in same directory
pub fn create_delete() {
    const N: usize = 20;
    const NCHILD: usize = 4;
    let mut name = [0; 32];

    for pi in 0..NCHILD {
        process::fork_fn(|| {
            name[0] = b'p' + u8::try_from(pi).unwrap();
            name[2] = b'\0';
            for i in 0..N {
                name[1] = b'0' + u8::try_from(i).unwrap();
                let path = CStr::from_bytes_until_nul(&name).unwrap();
                let file = File::create(path).unwrap();
                drop(file);
                if i > 0 && (i % 2) == 0 {
                    name[1] = b'0' + u8::try_from(i / 2).unwrap();
                    let path = CStr::from_bytes_until_nul(&name).unwrap();
                    fs::remove_file(path).unwrap();
                }
            }
            process::exit(0);
        })
        .unwrap();
    }

    for _ in 0..NCHILD {
        let (_, status) = process::wait().unwrap();
        assert!(status.success());
    }

    for i in 0..N {
        for pi in 0..NCHILD {
            name[0] = b'p' + u8::try_from(pi).unwrap();
            name[1] = b'0' + u8::try_from(i).unwrap();
            let path = CStr::from_bytes_until_nul(&name).unwrap();
            let file = File::open(path);
            assert!(
                (1..N / 2).contains(&i) || file.is_ok(),
                "oops create_delete {} didn't exist",
                path.to_str().unwrap()
            );
            assert!(
                !((1..N / 2).contains(&i) && file.is_ok()),
                "oops create_delete {} did exist",
                path.to_str().unwrap()
            )
        }
    }

    for i in 0..N {
        for pi in 0..NCHILD {
            name[0] = b'p' + u8::try_from(pi).unwrap();
            name[1] = b'0' + u8::try_from(i).unwrap();
            let path = CStr::from_bytes_until_nul(&name).unwrap();
            let _ = fs::remove_file(path);
        }
    }
}

/// can I unlink a file and still read it?
pub fn unlink_read() {
    const FILE_PATH: &CStr = c"unlinkread";

    File::create(FILE_PATH)
        .unwrap()
        .write_all(b"hello")
        .unwrap();

    let mut file1 = File::options()
        .create(true)
        .read(true)
        .write(true)
        .open(FILE_PATH)
        .unwrap();
    fs::remove_file(FILE_PATH).unwrap();

    let mut file2 = File::create(FILE_PATH).unwrap();
    file2.write_all(b"yyy").unwrap();
    drop(file2);

    let mut buf = [0; 5];
    file1.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"hello");
    file1.write_all(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap();
    drop(file1);

    fs::remove_file(FILE_PATH).unwrap();
}

pub fn link() {
    const FILE1_PATH: &CStr = c"lf1";
    const FILE2_PATH: &CStr = c"lf2";

    let mut file = File::create(FILE1_PATH).unwrap();
    file.write_all(b"hello").unwrap();
    drop(file);

    fs::link(FILE1_PATH, FILE2_PATH).unwrap();
    fs::remove_file(FILE1_PATH).unwrap();

    expect!(File::open(FILE1_PATH), Err(Ov6Error::FsEntryNotFound));

    let mut file = File::open(FILE2_PATH).unwrap();
    let mut buf = [0; 5];
    file.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"hello");
    drop(file);

    expect!(
        fs::link(FILE2_PATH, FILE2_PATH),
        Err(Ov6Error::AlreadyExists)
    );

    fs::remove_file(FILE2_PATH).unwrap();
    expect!(
        fs::link(FILE2_PATH, FILE1_PATH),
        Err(Ov6Error::FsEntryNotFound)
    );

    expect!(fs::link(c".", FILE1_PATH), Err(Ov6Error::NotADirectory));
}

/// test concurrent create/link/unlink of the same file
pub fn concreate() {
    const N: usize = 40;

    let mut file = [0; 3];
    file[0] = b'C';
    file[2] = b'\0';
    for i in 0..N {
        file[1] = b'0' + u8::try_from(i).unwrap();
        let path = CStr::from_bytes_until_nul(&file).unwrap();
        let _ = fs::remove_file(path);

        let res = process::fork().unwrap();
        if (res.is_parent() && (i % 3) == 1) || (res.is_child() && (i % 5) == 1) {
            let _ = fs::link(c"C0", path);
        } else {
            let file = File::create(path).unwrap();
            drop(file);
        }

        if res.is_child() {
            process::exit(0);
        }
        let (_, status) = process::wait().unwrap();
        assert!(status.success());
    }

    let mut n = 0;
    let mut fa = [0; N];
    let dir = fs::read_dir(c".").unwrap();
    for entry in dir {
        let entry = entry.unwrap();
        if entry.name().len() == 2 && entry.name().as_bytes()[0] == b'C' {
            let i = usize::from(entry.name().as_bytes()[1] - b'0');
            assert!(fa[i] == 0);
            fa[i] = 1;
            n += 1;
        }
    }
    assert_eq!(n, N);

    for i in 0..N {
        file[1] = b'0' + u8::try_from(i).unwrap();
        let path = CStr::from_bytes_until_nul(&file).unwrap();

        let res = process::fork().unwrap();
        if ((i % 3) == 0 && res.is_child()) || ((i % 3) == 1 && res.is_parent()) {
            let _ = File::open(path);
            let _ = File::open(path);
            let _ = File::open(path);
            let _ = File::open(path);
            let _ = File::open(path);
            let _ = File::open(path);
        } else {
            let _ = fs::remove_file(path);
            let _ = fs::remove_file(path);
            let _ = fs::remove_file(path);
            let _ = fs::remove_file(path);
            let _ = fs::remove_file(path);
            let _ = fs::remove_file(path);
        }
        if res.is_child() {
            process::exit(0);
        }
        process::wait().unwrap();
    }
}

/// another concurrent link/unlink/create test,
/// to look for deadlocks.
pub fn link_unlink() {
    let _ = fs::remove_file(c"x");

    let res = process::fork().unwrap();

    let mut x: u32 = if res.is_parent() { 1 } else { 97 };
    for _ in 0..100 {
        x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        match x % 3 {
            0 => {
                let _ = File::options().create(true).open(c"x");
            }
            1 => {
                let _ = fs::link(c"cat", c"x");
            }
            _ => {
                let _ = fs::remove_file(c"x");
            }
        }
    }

    if res.is_child() {
        process::exit(0);
    }
    let (_, status) = process::wait().unwrap();
    assert!(status.success());

    let _ = fs::remove_file(c"x");
}

pub fn subdir() {
    let _ = fs::remove_file(c"ff");
    fs::create_dir(c"dd").unwrap();

    let mut file = File::create(c"dd/ff").unwrap();
    file.write_all(b"ff").unwrap();
    drop(file);

    expect!(fs::remove_file(c"dd"), Err(Ov6Error::DirectoryNotEmpty));

    fs::create_dir(c"/dd/dd").unwrap();

    let mut file = File::create(c"dd/dd/ff").unwrap();
    file.write_all(b"FF").unwrap();
    drop(file);

    let mut file = File::open(c"dd/dd/../ff").unwrap();
    let mut buf = [0; 2];
    file.read_exact(&mut buf).unwrap();
    assert_eq!(buf, *b"ff");
    drop(file);

    fs::link(c"dd/dd/ff", c"dd/dd/ffff").unwrap();
    fs::remove_file(c"dd/dd/ff").unwrap();
    expect!(File::open(c"dd/dd/ff"), Err(Ov6Error::FsEntryNotFound));

    env::set_current_directory(c"dd").unwrap();
    env::set_current_directory(c"dd/../../dd").unwrap();
    env::set_current_directory(c"dd/../../../dd").unwrap();
    env::set_current_directory(c"./..").unwrap();

    let mut file = File::open(c"dd/dd/ffff").unwrap();
    let mut buf = [0; 2];
    file.read_exact(&mut buf).unwrap();
    drop(file);

    expect!(File::open(c"dd/dd/ff"), Err(Ov6Error::FsEntryNotFound));

    expect!(File::create(c"dd/ff/ff"), Err(Ov6Error::NotADirectory));
    expect!(File::create(c"dd/xx/ff"), Err(Ov6Error::FsEntryNotFound));
    expect!(
        File::options().create(true).read(true).open(c"dd"),
        Err(Ov6Error::AlreadyExists)
    );
    expect!(
        File::options().read(true).write(true).open(c"dd"),
        Err(Ov6Error::IsADirectory)
    );
    expect!(
        File::options().write(true).open(c"dd"),
        Err(Ov6Error::IsADirectory)
    );
    expect!(
        fs::link(c"dd/ff/ff", c"dd/dd/xx"),
        Err(Ov6Error::NotADirectory)
    );
    expect!(
        fs::link(c"dd/xx/ff", c"dd/dd/xx"),
        Err(Ov6Error::FsEntryNotFound)
    );
    expect!(
        fs::link(c"dd/ff", c"dd/dd/ffff"),
        Err(Ov6Error::AlreadyExists)
    );
    expect!(fs::create_dir(c"dd/ff/ff"), Err(Ov6Error::NotADirectory));
    expect!(fs::create_dir(c"dd/xx/ff"), Err(Ov6Error::FsEntryNotFound));
    expect!(fs::create_dir(c"dd/dd/ffff"), Err(Ov6Error::AlreadyExists));
    expect!(fs::remove_file(c"dd/xx/ff"), Err(Ov6Error::FsEntryNotFound));
    expect!(fs::remove_file(c"dd/ff/ff"), Err(Ov6Error::NotADirectory));
    expect!(
        env::set_current_directory(c"dd/ff"),
        Err(Ov6Error::NotADirectory)
    );
    expect!(
        env::set_current_directory(c"dd/xx"),
        Err(Ov6Error::FsEntryNotFound)
    );

    fs::remove_file(c"dd/dd/ffff").unwrap();
    fs::remove_file(c"dd/ff").unwrap();
    expect!(fs::remove_file(c"dd"), Err(Ov6Error::DirectoryNotEmpty));
    fs::remove_file(c"dd/dd").unwrap();
    fs::remove_file(c"dd").unwrap();
}

/// test writes that are larger than the log.
pub fn big_write() {
    const FILE_PATH: &CStr = c"bigwrite";
    let buf = unsafe { (&raw mut BUF).as_mut() }.unwrap();

    let _ = fs::remove_file(FILE_PATH);

    for size in (499..(MAX_OP_BLOCKS + 2) * FS_BLOCK_SIZE).step_by(471) {
        let mut file = File::create(FILE_PATH).unwrap();
        for _ in 0..2 {
            file.write_all(&buf[..size]).unwrap();
        }
        drop(file);
        fs::remove_file(FILE_PATH).unwrap();
    }
}

pub fn big_file() {
    const FILE_PATH: &CStr = c"bigfile.dat";
    const N: usize = 20;
    const SIZE: usize = 600;

    let buf = unsafe { (&raw mut BUF).as_mut() }.unwrap();
    let buf = &mut buf[..SIZE];

    let _ = fs::remove_file(FILE_PATH);
    let mut file = File::create(FILE_PATH).unwrap();
    for i in 0..N {
        buf.fill(i.try_into().unwrap());
        file.write(buf).unwrap();
    }
    drop(file);

    let mut file = File::open(FILE_PATH).unwrap();
    let mut total = 0;
    for i in 0.. {
        let n = file.read(&mut buf[..SIZE / 2]).unwrap();
        if n == 0 {
            break;
        }
        assert_eq!(n, SIZE / 2);
        assert!(buf[0] == i / 2 && buf[SIZE / 2 - 1] == i / 2);
        total += n;
    }
    drop(file);
    assert_eq!(total, N * SIZE);
    fs::remove_file(FILE_PATH).unwrap();
}

pub fn fourteen() {
    // DIR_SIZE is 14

    const N14: &CStr = c"12345678901234";
    const N14_15: &CStr = c"12345678901234/123456789012345";
    const N15_15_15: &CStr = c"123456789012345/123456789012345/123456789012345";
    const N14_14_14: &CStr = c"12345678901234/12345678901234/12345678901234";
    const N14_14: &CStr = c"12345678901234/12345678901234";
    const N15_14: &CStr = c"123456789012345/12345678901234";

    fs::create_dir(N14).unwrap();
    fs::create_dir(N14_15).unwrap();
    let _ = File::create(N15_15_15).unwrap();
    let _ = File::open(N14_14_14).unwrap();
    expect!(fs::create_dir(N14_14), Err(Ov6Error::AlreadyExists));
    expect!(fs::create_dir(N15_14), Err(Ov6Error::AlreadyExists));

    // clean up
    expect!(fs::remove_file(N15_14), Err(Ov6Error::DirectoryNotEmpty));
    expect!(fs::remove_file(N14_14), Err(Ov6Error::DirectoryNotEmpty));
    fs::remove_file(N14_14_14).unwrap();
    expect!(fs::remove_file(N15_15_15), Err(Ov6Error::FsEntryNotFound));
    fs::remove_file(N14_15).unwrap();
    fs::remove_file(N14).unwrap();
}

pub fn rm_dot() {
    fs::create_dir(c"dots").unwrap();
    env::set_current_directory(c"dots").unwrap();
    expect!(fs::remove_file(c"."), Err(Ov6Error::Unknown));
    expect!(fs::remove_file(c".."), Err(Ov6Error::Unknown));

    env::set_current_directory(c"/").unwrap();
    expect!(fs::remove_file(c"dots/."), Err(Ov6Error::Unknown));
    expect!(fs::remove_file(c"dots/.."), Err(Ov6Error::Unknown));
    fs::remove_file(c"dots").unwrap();
}

pub fn dir_file() {
    let _file = File::create(c"dirfile").unwrap();
    expect!(
        env::set_current_directory(c"dirfile"),
        Err(Ov6Error::NotADirectory)
    );
    expect!(File::open(c"dirfile/xx"), Err(Ov6Error::NotADirectory));
    expect!(File::create(c"dirfile/xx"), Err(Ov6Error::NotADirectory));
    expect!(fs::create_dir(c"dirfile/xx"), Err(Ov6Error::NotADirectory));
    expect!(fs::remove_file(c"dirfile/xx"), Err(Ov6Error::NotADirectory));
    expect!(
        fs::link(README_PATH, c"dirfile/xx"),
        Err(Ov6Error::NotADirectory)
    );
    fs::remove_file(c"dirfile").unwrap();

    expect!(
        File::options().read(true).write(true).open(c"."),
        Err(Ov6Error::IsADirectory),
    );
    let mut file = File::open(c".").unwrap();
    expect!(file.write(b"x"), Err(Ov6Error::BadFileDescriptor));
}

/// test that `inode_put()` is called at the end of `_namei()`.
/// also tests empty file names.
pub fn iref() {
    const DIR_PATH: &CStr = c"irefd";
    for _ in 0..=NINODE {
        fs::create_dir(DIR_PATH).unwrap();
        env::set_current_directory(DIR_PATH).unwrap();

        expect!(fs::create_dir(c""), Err(Ov6Error::AlreadyExists));
        expect!(fs::link(c"README", c""), Err(Ov6Error::AlreadyExists));
        let _ = File::open(c"").unwrap();
        let _file = File::create(c"xx").unwrap();
        fs::remove_file(c"xx").unwrap();
    }

    // clean up
    for _ in 0..=NINODE {
        env::set_current_directory(c"..").unwrap();
        fs::remove_file(DIR_PATH).unwrap();
    }

    env::set_current_directory(c"/").unwrap();
}
