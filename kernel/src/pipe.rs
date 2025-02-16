use core::ptr::NonNull;

use crate::{
    file::{self, File},
    kalloc,
    proc::{self, Proc},
    sync::SpinLock,
    vm::{self, VirtAddr},
};

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
        kalloc::free_page(pipe.cast());
    }
}

pub fn write(pipe: &Pipe, addr: VirtAddr, n: usize) -> Result<usize, ()> {
    let p = Proc::current();
    let mut i = 0;

    let mut pipe = pipe.data.lock();
    while i < n {
        if !pipe.readopen || p.killed() {
            return Err(());
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

pub fn read(pipe: &Pipe, addr: VirtAddr, n: usize) -> Result<usize, ()> {
    let p = Proc::current();

    let mut pipe = pipe.data.lock();
    while pipe.nread == pipe.nwrite && pipe.writeopen {
        if p.killed() {
            return Err(());
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
