use ov6_syscall::{RegisterDecodeError, error::SyscallError};
use ov6_types::{fs::RawFd, process::ProcId};

use crate::memory::VirtAddr;

#[derive(Debug, thiserror::Error)]
pub enum KernelError {
    #[error("no free process found")]
    NoFreeProc,
    #[error("no free page found")]
    NoFreePage,
    #[error("no child process")]
    NoChildProcess,
    #[error("process not found: {0}")]
    ProcessNotFound(ProcId),
    #[error("too large virtual address: {0:#x}")]
    TooLargeVirtualAddress(VirtAddr),
    #[error("address not mapped: {0:#x}")]
    AddressNotMapped(VirtAddr),
    #[error("inaccessible memory: {0:#x}")]
    InaccessibleMemory(VirtAddr),
    #[error("unterminated string: addr={0:#x}, len={1}")]
    UnterminatedString(VirtAddr, usize),
    #[error("bad file descriptor: fd={0}, pid={1}")]
    BadFileDescriptor(RawFd, ProcId),
    #[error("broken pipe")]
    BrokenPipe,
    #[error("sycall decode: {0}")]
    SyscallDecode(#[from] RegisterDecodeError),
    #[error("caller process already killed")]
    CallerProcessAlreadyKilled,
    #[error("unknown error")]
    Unknown,
}

impl From<KernelError> for SyscallError {
    fn from(error: KernelError) -> Self {
        match error {
            KernelError::NoFreeProc => Self::ResourceTempolaryUnavailable,
            KernelError::NoFreePage => Self::OutOfMemory,
            KernelError::ProcessNotFound(_) => Self::ProcessNotFound,
            KernelError::NoChildProcess => Self::NoChildProcess,
            KernelError::TooLargeVirtualAddress(_)
            | KernelError::AddressNotMapped(_)
            | KernelError::InaccessibleMemory(_)
            | KernelError::UnterminatedString(_, _) => Self::BadAddress,
            KernelError::BadFileDescriptor(_, _) => Self::BadFileDescriptor,
            KernelError::BrokenPipe => Self::BrokenPipe,
            KernelError::SyscallDecode(_)
            | KernelError::CallerProcessAlreadyKilled
            | KernelError::Unknown => Self::Unknown,
        }
    }
}
