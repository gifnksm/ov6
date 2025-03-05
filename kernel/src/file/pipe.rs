use alloc::sync::Arc;

use crate::{
    error::Error,
    memory::{VirtAddr, page::PageFrameAllocator, vm},
    proc::{Proc, ProcPrivateData},
    sync::{SpinLock, SpinLockCondVar},
};

use super::{File, FileData, FileDataArc, SpecificData};

const PIPE_SIZE: usize = 512;

#[derive(Clone)]
pub(super) struct PipeFile(Arc<PipeData, PageFrameAllocator>);

struct PipeData {
    reader_cond: SpinLockCondVar,
    writer_cond: SpinLockCondVar,
    data: SpinLock<PipeDataLocked>,
}

struct PipeDataLocked {
    data: [u8; PIPE_SIZE],
    /// Number of bytes read
    nread: usize,
    /// Number of bytes written
    nwrite: usize,
    /// read fd is still open
    read_open: bool,
    /// write fd is still open
    write_open: bool,
}

pub(super) fn new_file() -> Result<(File, File), Error> {
    let pipe = PipeFile(Arc::new_in(
        PipeData {
            reader_cond: SpinLockCondVar::new(),
            writer_cond: SpinLockCondVar::new(),
            data: SpinLock::new(PipeDataLocked {
                data: [0; PIPE_SIZE],
                nread: 0,
                nwrite: 0,
                read_open: true,
                write_open: true,
            }),
        },
        PageFrameAllocator,
    ));

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
        let mut pi = self.0.data.lock();
        if writable {
            pi.write_open = false;
            self.0.reader_cond.notify();
        } else {
            pi.read_open = false;
            self.0.writer_cond.notify();
        }
    }

    pub(super) fn write(
        &self,
        p: &Proc,
        private: &ProcPrivateData,
        addr: VirtAddr,
        n: usize,
    ) -> Result<usize, Error> {
        let mut i = 0;

        let mut pipe = self.0.data.lock();
        while i < n {
            if !pipe.read_open || p.shared().lock().killed() {
                return Err(Error::Unknown);
            }
            if pipe.nwrite == pipe.nread + PIPE_SIZE {
                self.0.reader_cond.notify();
                pipe = self.0.writer_cond.wait(pipe);
                continue;
            }

            let Ok(byte) = vm::copy_in(private.pagetable().unwrap(), addr.byte_add(i)) else {
                break;
            };
            let idx = pipe.nwrite % PIPE_SIZE;
            pipe.data[idx] = byte;
            pipe.nwrite += 1;
            i += 1;
        }
        self.0.reader_cond.notify();
        Ok(i)
    }

    pub(super) fn read(
        &self,
        p: &Proc,
        private: &mut ProcPrivateData,
        addr: VirtAddr,
        n: usize,
    ) -> Result<usize, Error> {
        let mut pipe = self.0.data.lock();
        while pipe.nread == pipe.nwrite && pipe.write_open {
            if p.shared().lock().killed() {
                return Err(Error::Unknown);
            }
            pipe = self.0.reader_cond.wait(pipe);
        }
        let mut i = 0;
        while i < n {
            if pipe.nread == pipe.nwrite {
                break;
            }
            let ch = pipe.data[pipe.nread % PIPE_SIZE];
            pipe.nread += 1;
            if vm::copy_out(private.pagetable_mut().unwrap(), addr.byte_add(i), &ch).is_err() {
                break;
            }
            i += 1;
        }
        self.0.writer_cond.notify();
        Ok(i)
    }
}
