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
    FileDescriptorNotFound(RawFd, ProcId),
    #[error("file descriptor not readable")]
    FileDescriptorNotReadable,
    #[error("file descriptor not writable")]
    FileDescriptorNotWritable,
    #[error("non-directory component in path")]
    NonDirectoryPathComponent,
    #[error("file system entry not found")]
    FsEntryNotFound,
    #[error("directory not empty")]
    DirectoryNotEmpty,
    #[error("unlink root directory")]
    UnlinkRootDir,
    #[error("create root directory")]
    CreateRootDir,
    #[error("create already exist entry")]
    CreateAlreadyExists,
    #[error("link root directory")]
    LinkRootDir,
    #[error("link cross devices")]
    LinkCrossDevices,
    #[error("link to non-directory")]
    LinkToNonDirectory,
    #[error("link already exists entry")]
    LinkAlreadyExists,
    #[error("broken pipe")]
    BrokenPipe,
    #[error("file too large")]
    FileTooLarge,
    #[error("too many open files in system")]
    TooManyOpenFilesSystem,
    #[error("too many open files")]
    TooManyOpenFiles,
    #[error("storage out of blocks")]
    StorageOutOfBlocks,
    #[error("storage out of inodes")]
    StorageOutOfInodes,
    #[error("open directory as writable")]
    OpenDirAsWritable,
    #[error("chdir to non-directory")]
    ChdirNotDir,
    #[error("argument list too long")]
    ArgumentListTooLong,
    #[error("invalid executable")]
    InvalidExecutable,
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
            KernelError::FileDescriptorNotFound(_, _)
            | KernelError::FileDescriptorNotReadable
            | KernelError::FileDescriptorNotWritable => Self::BadFileDescriptor,
            KernelError::NonDirectoryPathComponent
            | KernelError::ChdirNotDir
            | KernelError::LinkToNonDirectory => Self::NotADirectory,
            KernelError::FsEntryNotFound => Self::FsEntryNotFound,
            KernelError::DirectoryNotEmpty => Self::DirectoryNotEmpty,
            KernelError::UnlinkRootDir => Self::ResourceBusy,
            KernelError::CreateRootDir
            | KernelError::CreateAlreadyExists
            | KernelError::LinkRootDir
            | KernelError::LinkAlreadyExists => Self::AlreadyExists,
            KernelError::LinkCrossDevices => Self::CrossesDevices,
            KernelError::BrokenPipe => Self::BrokenPipe,
            KernelError::FileTooLarge => Self::FileTooLarge,
            KernelError::TooManyOpenFilesSystem => Self::TooManyOpenFilesSystem,
            KernelError::TooManyOpenFiles => Self::TooManyOpenFiles,
            KernelError::StorageOutOfBlocks | KernelError::StorageOutOfInodes => Self::StorageFull,
            KernelError::OpenDirAsWritable => Self::IsADirectory,
            KernelError::ArgumentListTooLong => Self::ArgumentListTooLong,
            KernelError::InvalidExecutable => Self::ExecFormat,
            KernelError::SyscallDecode(_)
            | KernelError::CallerProcessAlreadyKilled
            | KernelError::Unknown => Self::Unknown,
        }
    }
}
