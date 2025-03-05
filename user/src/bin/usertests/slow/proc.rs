use core::ptr;

use ov6_user_lib::{io::STDOUT_FD, os::ov6::syscall, process};

use crate::{ECHO_PATH, PAGE_SIZE};

/// test the exec() code that cleans up if it runs out
/// of memory. it's really a test that such a condition
/// doesn't cause a panic.
pub fn execout() {
    for avail in 0..15 {
        let status = process::fork_fn(|| {
            // allocate all of memory.
            loop {
                let Ok(a) = process::grow_break(PAGE_SIZE) else {
                    break;
                };
                unsafe { a.add(PAGE_SIZE - 1).write_volatile(1) };
            }

            // free a few pages, in order to let exec() make some
            // progress.
            for _ in 0..avail {
                unsafe { process::shrink_break(PAGE_SIZE) }.unwrap();
            }

            unsafe { syscall::close(STDOUT_FD) }.unwrap();
            let args = [ECHO_PATH.as_ptr(), c"x".as_ptr(), ptr::null()];
            let _ = process::exec(ECHO_PATH, &args);
            process::exit(0);
        })
        .unwrap()
        .wait()
        .unwrap();
        assert!(status.success() || status.code() == -1);
    }
}
