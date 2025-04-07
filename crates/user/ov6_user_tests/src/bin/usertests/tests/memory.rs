use ov6_syscall::{UserMutSlice, UserSlice, error::SyscallError, syscall};
use ov6_user_lib::{
    fs::{self, File},
    io::{self, Read as _, STDOUT_FD, Write as _},
    os::{fd::AsRawFd as _, ov6::syscall::ffi::SyscallExt as _},
    pipe,
    process::{self, Stdio},
};
use ov6_user_tests::expect;

use crate::README_PATH;

const FILE_PATH: &str = "copyin1";

/// what if you pass ridiculous pointers to system calls
/// that read user memory with copyin?
pub fn copy_u2k() {
    let addrs: &[usize] = &[
        0x8000_0000,
        0x3f_ffff_e000,
        0x3f_ffff_f000,
        0x40_0000_0000,
        0xffff_ffff_ffff_ffff,
    ];

    for &addr in addrs {
        let file = File::create(FILE_PATH).unwrap();
        expect!(
            syscall::Write::call((file.as_raw_fd(), unsafe {
                UserSlice::from_raw_parts(addr, 8192)
            })),
            Err(SyscallError::BadAddress),
            "addr={addr:?}",
        );
        drop(file);
        fs::remove_file(FILE_PATH).unwrap();

        expect!(
            syscall::Write::call((STDOUT_FD, unsafe { UserSlice::from_raw_parts(addr, 8192) })),
            Err(SyscallError::BadAddress),
            "addr={addr:?}",
        );

        let (_rx, tx) = pipe::pipe().unwrap();
        expect!(
            syscall::Write::call((tx.as_raw_fd(), unsafe {
                UserSlice::from_raw_parts(addr, 8192)
            })),
            Err(SyscallError::BadAddress),
            "addr={addr:?}",
        );
    }
}

/// what if you pass ridiculous pointers to system calls
/// that write user memory with copyout?
pub fn copy_k2u() {
    let addrs: &[usize] = &[
        0,
        0x8000_0000,
        0x3f_ffff_e000,
        0x3f_ffff_f000,
        0x40_0000_0000,
        0xffff_ffff_ffff_ffff,
    ];

    for &addr in addrs {
        let file = File::open(README_PATH).unwrap();

        expect!(
            syscall::Read::call((file.as_raw_fd(), unsafe {
                UserMutSlice::from_raw_parts(addr, 8192)
            })),
            Err(SyscallError::BadAddress),
            "addr={addr:?}"
        );
        drop(file);

        let (rx, mut tx) = pipe::pipe().unwrap();
        tx.write_all(b"x").unwrap();
        expect!(
            syscall::Read::call((rx.as_raw_fd(), unsafe {
                UserMutSlice::from_raw_parts(addr, 8192)
            })),
            Err(SyscallError::BadAddress),
            "addr={addr:?}"
        );
    }
}

/// See if the kernel refuses to read/write user memory that the
/// application doesn't have anymore, because it returned it.
pub fn rw_sbrk() {
    let a = process::grow_break(8192).unwrap();
    let _ = unsafe { process::shrink_break(8192) }.unwrap();

    let file = File::create(FILE_PATH).unwrap();
    expect!(
        syscall::Write::call((file.as_raw_fd(), unsafe {
            UserSlice::from_raw_parts(a.addr() + 4096, 1024)
        })),
        Err(SyscallError::BadAddress),
    );
    drop(file);
    fs::remove_file(FILE_PATH).unwrap();

    let file = File::open(README_PATH).unwrap();
    expect!(
        syscall::Read::call((file.as_raw_fd(), unsafe {
            UserMutSlice::from_raw_parts(a.addr() + 4096, 10)
        })),
        Err(SyscallError::BadAddress),
    );
    drop(file);
}

/// Counts that the kernel can allocate and deallocate memory.
///
/// This uses `sbrt()` to count how many free physical memory pages there are.
/// Touches the pages to force allocation.
/// Because out of memory with lazy allocation results in the process
/// taking a fault and being killed, fork and report back.
pub fn count_free_pages() {
    let mut child = process::ProcessBuilder::new()
        .stdin(Stdio::Pipe)
        .stdout(Stdio::Pipe)
        .spawn_fn(|| {
            // wait until stdin closed
            assert_eq!(io::stdin().read(&mut [0]).unwrap(), 0);

            loop {
                unsafe {
                    let Ok(a) = process::grow_break(4096) else {
                        break;
                    };
                    // modify the memory to make sure it's really allocated.
                    a.add(4096 - 1).write(1);
                    // report back one more page.
                    io::stdout().write_all(b"x").unwrap();
                }
            }
            process::exit(0);
        })
        .unwrap();

    let tx = child.stdin.take().unwrap();
    // get free pages after fork, before sbrk
    let os_free_pages = ov6_user_lib::os::ov6::syscall::get_system_info()
        .unwrap()
        .memory
        .free_pages;
    drop(tx);

    let mut rx = child.stdout.take().unwrap();
    let mut available_pages = 0_usize;
    loop {
        let mut buf = [0];
        if rx.read(&mut buf).unwrap() == 0 {
            break;
        }
        available_pages += 1;
    }
    drop(rx);
    let exit_status = child.wait().unwrap();
    assert!(exit_status.success());

    let page_table_pages = available_pages / 512;

    assert!(
        (available_pages + page_table_pages).abs_diff(os_free_pages) < 2,
        "available_pages={available_pages}, page_table_pages={page_table_pages}, \
         os_free_pages={os_free_pages}"
    );
}
