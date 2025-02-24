use core::ptr::NonNull;

use crate::{
    error::Error,
    memory::{
        page,
        vm::{self, VirtAddr},
    },
    proc::{self, Proc},
    sync::SpinLock,
};

use super::File;

const PIPE_SIZE: usize = 512;

#[repr(C)]
pub struct Pipe {
    data: SpinLock<PipeData>,
}

#[repr(C)]
struct PipeData {
    data: [u8; PIPE_SIZE],
    /// Number of bytes read
    nread: usize,
    /// Number of bytes written
    nwrite: usize,
    /// read fd is still open
    readopen: bool,
    /// write fd is still open
    writeopen: bool,
}

pub fn alloc() -> Result<(&'static File, &'static File), Error> {
    let Ok(f0) = super::alloc().ok_or(()) else {
        return Err(Error::Unknown);
    };
    let Ok(f1) = super::alloc().ok_or(()) else {
        super::close(f0);
        return Err(Error::Unknown);
    };
    let Some(mut pi) = page::alloc_page().map(|p| p.cast::<Pipe>()) else {
        super::close(f0);
        super::close(f1);
        return Err(Error::Unknown);
    };

    unsafe {
        *pi.as_mut() = Pipe {
            data: SpinLock::new(PipeData {
                data: [0; PIPE_SIZE],
                nread: 0,
                nwrite: 0,
                readopen: true,
                writeopen: true,
            }),
        }
    };

    f0.init_read_pipe(pi);
    f1.init_write_pipe(pi);

    Ok((f0, f1))
}

pub fn close(pipe: NonNull<Pipe>, writable: bool) {
    let do_free;
    {
        let pi = unsafe { pipe.as_ref() };
        let mut pi = pi.data.lock();
        if writable {
            pi.writeopen = false;
            proc::wakeup((&raw const pi.nread).cast());
        } else {
            pi.readopen = false;
            proc::wakeup((&raw const pi.nwrite).cast());
        }
        do_free = !pi.readopen && !pi.writeopen;
    };
    if do_free {
        unsafe {
            page::free_page(pipe.cast());
        }
    }
}

pub fn write(pipe: &Pipe, addr: VirtAddr, n: usize) -> Result<usize, Error> {
    let p = Proc::current();
    let mut i = 0;

    let mut pipe = pipe.data.lock();
    while i < n {
        if !pipe.readopen || p.killed() {
            return Err(Error::Unknown);
        }
        if pipe.nwrite == pipe.nread + PIPE_SIZE {
            proc::wakeup((&raw const pipe.nread).cast());
            proc::sleep((&raw const pipe.nwrite).cast(), &mut pipe);
            continue;
        }

        let Ok(byte) = vm::copy_in(p.pagetable().unwrap(), addr.byte_add(i)) else {
            break;
        };
        let idx = pipe.nwrite % PIPE_SIZE;
        pipe.data[idx] = byte;
        pipe.nwrite += 1;
        i += 1;
    }
    proc::wakeup((&raw const pipe.nread).cast());
    Ok(i)
}

pub fn read(pipe: &Pipe, addr: VirtAddr, n: usize) -> Result<usize, Error> {
    let p = Proc::current();

    let mut pipe = pipe.data.lock();
    while pipe.nread == pipe.nwrite && pipe.writeopen {
        if p.killed() {
            return Err(Error::Unknown);
        }
        proc::sleep((&raw const pipe.nread).cast(), &mut pipe);
    }
    let mut i = 0;
    while i < n {
        if pipe.nread == pipe.nwrite {
            break;
        }
        let ch = pipe.data[pipe.nread % PIPE_SIZE];
        pipe.nread += 1;
        if vm::copy_out(p.pagetable().unwrap(), addr.byte_add(i), &ch).is_err() {
            break;
        }
        i += 1;
    }
    proc::wakeup((&raw const pipe.nwrite).cast());
    Ok(i)
}
