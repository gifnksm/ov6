use ov6_syscall::error::SyscallError;

#[derive(Debug, thiserror::Error)]
pub enum Ov6Error {
    // EPERM
    #[error("operation not permitted")]
    NotPermitted = 1,
    // ENOENT
    #[error("no such file or directory")]
    FsEntryNotFound = 2,
    // ESRCH
    #[error("no such process")]
    ProcessNotFound = 3,
    // #[error("interrupted system call")]
    // Interrupted,
    #[error("input/output error")]
    Io,
    #[error("no such device or address")]
    DeviceNotFound,
    #[error("argument list too long")]
    ArgumentListTooLong,
    #[error("exec format error")]
    ExecFormat,
    #[error("bad file descriptor")]
    BadFileDescriptor,
    #[error("no child process")]
    NoChildProcess,
    #[error("resource temporarily unavailable")]
    ResourceTempolaryUnavailable,
    #[error("cannot allocate memory")]
    OutOfMemory,
    #[error("permission denied")]
    PermissionDenied,
    #[error("bad address")]
    BadAddress,
    // #[error("block device required")]
    // BlockDeviceRequired,
    #[error("device or resource busy")]
    ResourceBusy,
    #[error("file exists")]
    AlreadyExists,
    #[error("cross-device link")]
    CrossesDevices,
    #[error("no such device")]
    NoSuchDevice,
    #[error("not a directory")]
    NotADirectory,
    #[error("is a directory")]
    IsADirectory,
    #[error("invalid argument")]
    InvalidInput,
    #[error("too many open files in system")]
    TooManyOpenFilesSystem,
    #[error("too many open files")]
    TooManyOpenFiles,
    // #[error("inappropriate I/O control operation")]
    // NoTty,
    #[error("text file busy")]
    ExecutableFileBusy,
    #[error("file too large")]
    FileTooLarge,
    #[error("no space left on device")]
    StorageFull,
    #[error("illegal seek")]
    NotSeekable,
    #[error("read-only file system")]
    ReadOnlyFilesystem,
    #[error("too many links")]
    TooManyLinks,
    #[error("broken pipe")]
    BrokenPipe,
    // #[error("math argument out of domain of func")]
    // MathOutOfDOmain,
    // #[error("math result not representable")]
    // MathNotRepresentable,
    // #[error("resource deadlock avoided")]
    // Deadlock,
    #[error("file name too long")]
    InvalidFilename,
    // #[error("resource deadlock avoided")]
    // Deadlock,
    // #[error("function not implemented")]
    // FunctionNotImplemented,
    #[error("directory not empty")]
    DirectoryNotEmpty,

    #[error("stream did not contain valid UTF-8")]
    InvalidUtf8,
    #[error("failed to fill whole buffer")]
    ReadExactEof,
    #[error("failed to write whole buffer")]
    WriteAllEof,
    #[error("failed to write the buffered data")]
    WriteZero,
    #[error("unknown error")]
    Unknown,
}

impl Ov6Error {
    #[must_use]
    pub fn is_interrupted(&self) -> bool {
        false // TODO
    }
}

impl From<SyscallError> for Ov6Error {
    fn from(error: SyscallError) -> Self {
        match error {
            SyscallError::NotPermitted => Self::NotPermitted,
            SyscallError::FsEntryNotFound => Self::FsEntryNotFound,
            SyscallError::ProcessNotFound => Self::ProcessNotFound,
            SyscallError::Io => Self::Io,
            SyscallError::DeviceNotFound => Self::DeviceNotFound,
            SyscallError::ArgumentListTooLong => Self::ArgumentListTooLong,
            SyscallError::ExecFormat => Self::ExecFormat,
            SyscallError::BadFileDescriptor => Self::BadFileDescriptor,
            SyscallError::NoChildProcess => Self::NoChildProcess,
            SyscallError::ResourceTempolaryUnavailable => Self::ResourceTempolaryUnavailable,
            SyscallError::OutOfMemory => Self::OutOfMemory,
            SyscallError::PermissionDenied => Self::PermissionDenied,
            SyscallError::BadAddress => Self::BadAddress,
            SyscallError::ResourceBusy => Self::ResourceBusy,
            SyscallError::AlreadyExists => Self::AlreadyExists,
            SyscallError::CrossesDevices => Self::CrossesDevices,
            SyscallError::NoSuchDevice => Self::NoSuchDevice,
            SyscallError::NotADirectory => Self::NotADirectory,
            SyscallError::IsADirectory => Self::IsADirectory,
            SyscallError::InvalidInput => Self::InvalidInput,
            SyscallError::TooManyOpenFilesSystem => Self::TooManyOpenFilesSystem,
            SyscallError::TooManyOpenFiles => Self::TooManyOpenFiles,
            SyscallError::ExecutableFileBusy => Self::ExecutableFileBusy,
            SyscallError::FileTooLarge => Self::FileTooLarge,
            SyscallError::StorageFull => Self::StorageFull,
            SyscallError::NotSeekable => Self::NotSeekable,
            SyscallError::ReadOnlyFilesystem => Self::ReadOnlyFilesystem,
            SyscallError::TooManyLinks => Self::TooManyLinks,
            SyscallError::BrokenPipe => Self::BrokenPipe,
            SyscallError::InvalidFilename => Self::InvalidFilename,
            SyscallError::DirectoryNotEmpty => Self::DirectoryNotEmpty,
            SyscallError::Unknown => Self::Unknown,
        }
    }
}
