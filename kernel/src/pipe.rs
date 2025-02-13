use core::ptr::NonNull;

use crate::{
    file::{self, File},
    kalloc,
    proc::{self, Proc},
    spinlock::SpinLock,
    vm::{self, VirtAddr},
};

mod ffi {
    use core::{ffi::c_int, ptr};

    use super::*;

    #[unsafe(no_mangle)]
    extern "C" fn pipealloc(f0: *mut *mut File, f1: *mut *mut File) -> c_int {
        match super::alloc() {
            Ok(res) => {
                unsafe {
                    *f0 = ptr::from_ref(res.0).cast_mut();
                    *f1 = ptr::from_ref(res.1).cast_mut();
                }
                0
            }
            Err(()) => -1,
        }
    }
}

const PIPE_SIZE: usize = 512;

#[repr(C)]
pub struct Pipe {
    lock: SpinLock,
    data: [u8; PIPE_SIZE],
    /// Number of bytes read
    nread: u32,
    /// Number of bytes written
    nwrite: u32,
    /// read fd is still open
    readopen: i32,
    /// write fd is still open
    writeopen: i32,
}

pub fn alloc() -> Result<(&'static File, &'static File), ()> {
    let Ok(f0) = file::alloc().ok_or(()) else {
        return Err(());
    };
    let Ok(f1) = file::alloc().ok_or(()) else {
        file::close(f0);
        return Err(());
    };
    let Some(mut pi) = kalloc::alloc_page().map(|p| p.cast::<Pipe>()) else {
        file::close(f0);
        file::close(f1);
        return Err(());
    };

    unsafe {
        *pi.as_mut() = Pipe {
            lock: SpinLock::new(c"pipe"),
            data: [0; PIPE_SIZE],
            nread: 0,
            nwrite: 0,
            readopen: 1,
            writeopen: 1,
        }
    };

    f0.init_read_pipe(pi);
    f1.init_write_pipe(pi);

    Ok((f0, f1))
}

pub fn close(mut pi: NonNull<Pipe>, writable: bool) {
    let do_free;
    {
        let pi = unsafe { pi.as_mut() };
        pi.lock.acquire();
        if writable {
            pi.writeopen = 0;
            proc::wakeup((&raw const pi.nread).cast());
        } else {
            pi.readopen = 0;
            proc::wakeup((&raw const pi.nwrite).cast());
        }
        do_free = pi.readopen == 0 && pi.writeopen == 0;
        pi.lock.release();
    };
    if do_free {
        kalloc::free_page(pi.cast());
    }
}

pub fn write(pi: &mut Pipe, addr: VirtAddr, n: usize) -> Result<usize, ()> {
    let pr = Proc::myproc().unwrap();
    let mut i = 0;

    pi.lock.acquire();
    while i < n {
        if pi.readopen == 0 || pr.killed() {
            pi.lock.release();
            return Err(());
        }
        if pi.nwrite as usize == (pi.nread as usize) + PIPE_SIZE {
            proc::wakeup((&raw const pi.nread).cast());
            proc::sleep_raw(pr, (&raw const pi.nwrite).cast(), &pi.lock);
            continue;
        }

        let mut byte: [u8; 1] = [0];
        if vm::copy_in(pr.pagetable().unwrap(), &mut byte, addr.byte_add(i)).is_err() {
            break;
        }
        pi.data[(pi.nwrite as usize) % PIPE_SIZE] = byte[0];
        pi.nwrite += 1;
        i += 1;
    }
    proc::wakeup((&raw const pi.nread).cast());
    pi.lock.release();
    Ok(i)
}

pub fn read(pi: &mut Pipe, addr: VirtAddr, n: usize) -> Result<usize, ()> {
    let pr = Proc::myproc().unwrap();

    pi.lock.acquire();
    while pi.nread == pi.nwrite && pi.writeopen != 0 {
        if pr.killed() {
            pi.lock.release();
            return Err(());
        }
        proc::sleep_raw(pr, (&raw const pi.nread).cast(), &pi.lock);
    }
    let mut i = 0;
    while i < n {
        if pi.nread == pi.nwrite {
            break;
        }
        let ch = [pi.data[pi.nread as usize % PIPE_SIZE]];
        pi.nread += 1;
        if vm::copy_out(pr.pagetable().unwrap(), addr.byte_add(i), &ch).is_err() {
            break;
        }
        i += 1;
    }
    proc::wakeup((&raw const pi.nwrite).cast());
    pi.lock.release();
    Ok(i)
}
