use alloc::sync::Arc;

use crate::{
    error::Error,
    memory::{
        page::PageFrameAllocator,
        vm::{self, VirtAddr},
    },
    proc::{self, Proc},
    sync::SpinLock,
};

use super::{File, FileData, FileDataArc, SpecificData};

const PIPE_SIZE: usize = 512;

#[derive(Clone)]
pub(super) struct PipeFile {
    data: Arc<SpinLock<PipeData>, PageFrameAllocator>,
}

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

pub(super) fn new_file() -> Result<(File, File), Error> {
    let pipe = PipeFile {
        data: Arc::new_in(
            SpinLock::new(PipeData {
                data: [0; PIPE_SIZE],
                nread: 0,
                nwrite: 0,
                readopen: true,
                writeopen: true,
            }),
            PageFrameAllocator,
        ),
    };

    let f0 = File {
        data: FileDataArc::try_new(FileData {
            readable: true,
            writable: false,
            data: Some(SpecificData::Pipe(pipe.clone())),
        })?,
    };
    let f1 = File {
        data: FileDataArc::try_new(FileData {
            readable: false,
            writable: true,
            data: Some(SpecificData::Pipe(pipe)),
        })?,
    };

    Ok((f0, f1))
}

impl PipeFile {
    pub(super) fn close(&self, writable: bool) {
        let mut pi = self.data.lock();
        if writable {
            pi.writeopen = false;
            proc::wakeup((&raw const pi.nread).cast());
        } else {
            pi.readopen = false;
            proc::wakeup((&raw const pi.nwrite).cast());
        }
    }

    pub(super) fn write(&self, addr: VirtAddr, n: usize) -> Result<usize, Error> {
        let p = Proc::current();
        let mut i = 0;

        let mut pipe = self.data.lock();
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

    pub(super) fn read(&self, addr: VirtAddr, n: usize) -> Result<usize, Error> {
        let p = Proc::current();

        let mut pipe = self.data.lock();
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
}
