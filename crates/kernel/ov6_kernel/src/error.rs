use ov6_fs_types::InodeNo;
use ov6_syscall::{RegisterDecodeError, error::SyscallError};
use ov6_types::{fs::RawFd, process::ProcId};

use crate::{
    fs::DeviceNo,
    memory::{VirtAddr, page_table::PtEntryFlags},
    sync::{SleepLockError, WaitError},
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum KernelError {
    #[error("no free process found")]
    NoFreeProc,
    #[error("no free page found")]
    NoFreePage,
    #[error("no child process")]
    NoWaitTarget,
    #[error("process not found: {0}")]
    ProcessNotFound(ProcId),
    #[error("device not found: {0}")]
    DeviceNotFound(DeviceNo),
    #[error("too large virtual address: {0:#x}")]
    TooLargeVirtualAddress(usize),
    #[error("virtual address underflow")]
    VirtualAddressUnderflow,
    #[error("page not mapped: {0:#x}")]
    VirtualPageNotMapped(VirtAddr),
    #[error("inaccessible page: {0:#x}")]
    InaccessiblePage(VirtAddr),
    #[error("virtual address with different permission: va={0:#x}, flags={1:?},{2:?}")]
    VirtualAddressWithUnexpectedPerm(VirtAddr, PtEntryFlags, PtEntryFlags),
    #[error("heap size overflow")]
    HeapSizeOverflow,
    #[error("heap size underflow")]
    HeapSizeUnderflow,
    #[error("bad file descriptor: fd={0}, pid={1}")]
    FileDescriptorNotFound(RawFd, ProcId),
    #[error("file descriptor not readable")]
    FileDescriptorNotReadable,
    #[error("file descriptor not writable")]
    FileDescriptorNotWritable,
    #[error("path too long")]
    PathTooLong,
    #[error("null in path")]
    NullInPath,
    #[error("non-directory component in path")]
    NonDirectoryPathComponent,
    #[error("file system entry not found")]
    FsEntryNotFound,
    #[error("directory not empty")]
    DirectoryNotEmpty,
    #[error("write offset too large")]
    WriteOffsetTooLarge,
    #[error("unlink root directory")]
    UnlinkRootDir,
    #[error("unlink dot directories")]
    UnlinkDots,
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
    #[error("stat on non-file-system entry")]
    StatOnNonFsEntry,
    #[error("broken pipe")]
    BrokenPipe,
    #[error("file too large")]
    FileTooLarge,
    #[error("no free file table entry")]
    NoFreeFileTableEntry,
    #[error("no free file descriptor table entry")]
    NoFreeFileDescriptorTableEntry,
    #[error("no free inode in-memory table entry")]
    NoFreeInodeInMemoryTableEntry,
    #[error("no free inode data im-memory table entry")]
    NoFreeInodeDataInMemoryTableEntry,
    #[error("corraputed inode type: inode={0}, type={1}")]
    CorruptedInodeType(InodeNo, u16),
    #[error("storage out of blocks")]
    StorageOutOfBlocks,
    #[error("storage out of inodes")]
    StorageOutOfInodes,
    #[error("open directory as writable")]
    OpenDirAsWritable,
    #[error("chdir to non-directory")]
    ChdirNotDir,
    #[error("argument list too large")]
    ArgumentListTooLarge,
    #[error("invalid executable")]
    InvalidExecutable,
    #[error("sycall decode: {0}")]
    SyscallDecode(#[from] RegisterDecodeError),
    #[error("caller process already killed")]
    CallerProcessAlreadyKilled,
    #[error("caller not in signal handler")]
    NotInSignalHandler,
    #[error("too large UDP packet payload")]
    TooLargeUdpPacket,
    #[error("no send buffer available")]
    NoSendBuffer,
    #[error("port already bound")]
    PortAlreadyBound,
    #[error("no free port available")]
    NoFreePort,
    #[error("port not bound")]
    PortNotBound,
}

impl From<KernelError> for SyscallError {
    fn from(error: KernelError) -> Self {
        match error {
            KernelError::NoFreeProc | KernelError::NoSendBuffer | KernelError::NoFreePort => {
                Self::ResourceTempolaryUnavailable
            }
            KernelError::NoFreePage => Self::OutOfMemory,
            KernelError::ProcessNotFound(_) => Self::ProcessNotFound,
            KernelError::DeviceNotFound(_) => Self::DeviceNotFound,
            KernelError::NoWaitTarget => Self::NoChildProcess,
            KernelError::TooLargeVirtualAddress(_)
            | KernelError::VirtualAddressUnderflow
            | KernelError::VirtualPageNotMapped(_)
            | KernelError::InaccessiblePage(_)
            | KernelError::VirtualAddressWithUnexpectedPerm(_, _, _) => Self::BadAddress,
            KernelError::FileDescriptorNotFound(_, _)
            | KernelError::FileDescriptorNotReadable
            | KernelError::FileDescriptorNotWritable
            | KernelError::StatOnNonFsEntry => Self::BadFileDescriptor,
            KernelError::PathTooLong => Self::InvalidFilename,
            KernelError::NonDirectoryPathComponent
            | KernelError::ChdirNotDir
            | KernelError::LinkToNonDirectory => Self::NotADirectory,
            KernelError::FsEntryNotFound => Self::FsEntryNotFound,
            KernelError::DirectoryNotEmpty => Self::DirectoryNotEmpty,
            KernelError::WriteOffsetTooLarge => Self::NotSeekable,
            KernelError::UnlinkRootDir => Self::ResourceBusy,
            KernelError::HeapSizeOverflow
            | KernelError::HeapSizeUnderflow
            | KernelError::UnlinkDots
            | KernelError::NullInPath
            | KernelError::PortNotBound => Self::InvalidInput,
            KernelError::CreateRootDir
            | KernelError::CreateAlreadyExists
            | KernelError::LinkRootDir
            | KernelError::LinkAlreadyExists => Self::AlreadyExists,
            KernelError::LinkCrossDevices => Self::CrossesDevices,
            KernelError::BrokenPipe => Self::BrokenPipe,
            KernelError::FileTooLarge => Self::FileTooLarge,
            KernelError::NoFreeFileTableEntry
            | KernelError::NoFreeInodeInMemoryTableEntry
            | KernelError::NoFreeInodeDataInMemoryTableEntry => Self::TooManyOpenFilesSystem,
            KernelError::NoFreeFileDescriptorTableEntry => Self::TooManyOpenFiles,
            KernelError::CorruptedInodeType(_, _) => Self::Io,
            KernelError::StorageOutOfBlocks | KernelError::StorageOutOfInodes => Self::StorageFull,
            KernelError::OpenDirAsWritable => Self::IsADirectory,
            KernelError::ArgumentListTooLarge => Self::ArgumentListTooLong,
            KernelError::InvalidExecutable => Self::ExecFormat,
            KernelError::TooLargeUdpPacket => Self::MessageTooLong,
            KernelError::PortAlreadyBound => Self::AddrInUse,
            KernelError::NotInSignalHandler
            | KernelError::SyscallDecode(_)
            | KernelError::CallerProcessAlreadyKilled => Self::Unknown,
        }
    }
}

impl From<SleepLockError> for KernelError {
    fn from(error: SleepLockError) -> Self {
        match error {
            SleepLockError::LockingProcessAlreadyKilled => Self::CallerProcessAlreadyKilled,
        }
    }
}

impl From<WaitError> for KernelError {
    fn from(error: WaitError) -> Self {
        match error {
            WaitError::WaitingProcessAlreadyKilled => Self::CallerProcessAlreadyKilled,
        }
    }
}
