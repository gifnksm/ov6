use alloc::sync::Arc;

use ov6_syscall::{UserMutSlice, UserSlice};

use super::{File, FileData, FileDataArc, SpecificData};
use crate::{
    error::KernelError,
    memory::{page::PageFrameAllocator, vm_user::UserPageTable},
    proc::Proc,
    sync::{SpinLock, SpinLockCondVar},
};

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

pub(super) fn new_file() -> Result<(File, File), KernelError> {
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
        pt: &UserPageTable,
        src: UserSlice<u8>,
    ) -> Result<usize, KernelError> {
        let mut nwritten = 0;

        let mut pipe = self.0.data.lock();
        while nwritten < src.len() {
            if !pipe.read_open {
                if nwritten > 0 {
                    break;
                }
                return Err(KernelError::BrokenPipe);
            }
            if p.shared().lock().killed() {
                return Err(KernelError::CallerProcessAlreadyKilled);
            }
            if pipe.nwrite == pipe.nread + PIPE_SIZE {
                self.0.reader_cond.notify();
                pipe = self.0.writer_cond.wait(pipe);
                continue;
            }

            let mut byte = [0];
            if let Err(e) = pt.copy_in_bytes(&mut byte, src.skip(nwritten).take(1)) {
                if nwritten > 0 {
                    break;
                }
                return Err(e);
            }

            let idx = pipe.nwrite % PIPE_SIZE;
            pipe.data[idx] = byte[0];
            pipe.nwrite += 1;
            nwritten += 1;
        }
        self.0.reader_cond.notify();
        Ok(nwritten)
    }

    pub(super) fn read(
        &self,
        p: &Proc,
        pt: &mut UserPageTable,
        dst: UserMutSlice<u8>,
    ) -> Result<usize, KernelError> {
        let mut pipe = self.0.data.lock();
        while pipe.nread == pipe.nwrite && pipe.write_open {
            if p.shared().lock().killed() {
                return Err(KernelError::CallerProcessAlreadyKilled);
            }
            pipe = self.0.reader_cond.wait(pipe);
        }
        let mut nread = 0;
        while nread < dst.len() {
            if pipe.nread == pipe.nwrite {
                break;
            }
            let ch = pipe.data[pipe.nread % PIPE_SIZE];
            pipe.nread += 1;

            if let Err(e) = pt.copy_out_bytes(dst.skip_mut(nread).take_mut(1), &[ch]) {
                if nread > 0 {
                    break;
                }
                return Err(e);
            }
            nread += 1;
        }
        self.0.writer_cond.notify();
        Ok(nread)
    }
}
