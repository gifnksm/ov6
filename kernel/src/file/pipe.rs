use alloc::sync::Arc;

use ov6_syscall::{UserMutSlice, UserSlice};

use super::{File, FileData, FileDataArc, SpecificData};
use crate::{
    error::KernelError,
    memory::{addr::Validated, page::PageFrameAllocator, vm_user::UserPageTable},
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
        pt: &UserPageTable,
        src: &Validated<UserSlice<u8>>,
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
            if pipe.nwrite == pipe.nread + PIPE_SIZE {
                self.0.reader_cond.notify();
                pipe = self.0.writer_cond.wait(pipe).map_err(|(_guard, e)| e)?;
                continue;
            }

            let mut byte = [0];
            pt.copy_in_bytes(&mut byte, &src.skip(nwritten).take(1));

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
        pt: &mut UserPageTable,
        dst: &mut Validated<UserMutSlice<u8>>,
    ) -> Result<usize, KernelError> {
        let mut pipe = self.0.data.lock();
        while pipe.nread == pipe.nwrite && pipe.write_open {
            pipe = self.0.reader_cond.wait(pipe).map_err(|(_guard, e)| e)?;
        }
        let mut nread = 0;
        while nread < dst.len() {
            if pipe.nread == pipe.nwrite {
                break;
            }
            let ch = pipe.data[pipe.nread % PIPE_SIZE];
            pipe.nread += 1;

            pt.copy_out_bytes(&mut dst.skip_mut(nread).take_mut(1), &[ch]);
            nread += 1;
        }
        self.0.writer_cond.notify();
        Ok(nread)
    }
}
