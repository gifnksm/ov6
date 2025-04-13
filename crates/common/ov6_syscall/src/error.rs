use strum::FromRepr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRepr, thiserror::Error)]
#[repr(isize)]
pub enum SyscallError {
    // EPERM
    #[error("operation not permitted")]
    NotPermitted = 1,
    // ENOENT
    #[error("no such file or directory")]
    FsEntryNotFound = 2,
    // ESRCH
    #[error("no such process")]
    ProcessNotFound = 3,
    // // EINTR
    // #[error("interrupted system call")]
    // Interrupted = 4,
    // EIO
    #[error("input/output error")]
    Io = 5,
    // ENXIO
    #[error("no such device or address")]
    DeviceNotFound = 6,
    // E2BIG
    #[error("argument list too long")]
    ArgumentListTooLong = 7,
    // ENOEXEC
    #[error("exec format error")]
    ExecFormat = 8,
    // EBADF
    #[error("bad file descriptor")]
    BadFileDescriptor = 9,
    // ECHILD
    #[error("no child process")]
    NoChildProcess = 10,
    // EAGAIN
    #[error("resource temporarily unavailable")]
    ResourceTempolaryUnavailable = 11,
    // ENOMEM
    #[error("cannot allocate memory")]
    OutOfMemory = 12,
    // EACCESS
    #[error("permission denied")]
    PermissionDenied = 13,
    // EFAULT
    #[error("bad address")]
    BadAddress = 14,
    // // ENOTBLK
    // #[error("block device required")]
    // BlockDeviceRequired = 15,
    // EBUSY
    #[error("device or resource busy")]
    ResourceBusy = 16,
    // EEXIST
    #[error("file exists")]
    AlreadyExists = 17,
    // EXDEV
    #[error("cross-device link")]
    CrossesDevices = 18,
    // ENODEV
    #[error("no such device")]
    NoSuchDevice = 19,
    // ENOTDIR
    #[error("not a directory")]
    NotADirectory = 20,
    // EISDIR
    #[error("is a directory")]
    IsADirectory = 21,
    // EINVAL
    #[error("invalid argument")]
    InvalidInput = 22,
    // ENFILE
    #[error("too many open files in system")]
    TooManyOpenFilesSystem = 23,
    // EMFILE
    #[error("too many open files")]
    TooManyOpenFiles = 24,
    // // ENOTTY
    // #[error("inappropriate I/O control operation")]
    // NoTty = 25,
    // ETXTBSY
    #[error("text file busy")]
    ExecutableFileBusy = 26,
    // EFBIG
    #[error("file too large")]
    FileTooLarge = 27,
    // ENOSPC
    #[error("no space left on device")]
    StorageFull = 28,
    // ESPIPE
    #[error("illegal seek")]
    NotSeekable = 29,
    // EROFS
    #[error("read-only file system")]
    ReadOnlyFilesystem = 30,
    // EMLINK
    #[error("too many links")]
    TooManyLinks = 31,
    // EPIPE
    #[error("broken pipe")]
    BrokenPipe = 32,
    // // EDOM
    // #[error("math argument out of domain of func")]
    // MathOutOfDOmain = 33,
    // // ERANGE
    // #[error("math result not representable")]
    // MathNotRepresentable = 34,
    // // EDEADLK
    // #[error("resource deadlock avoided")]
    // // Deadlock = 35,
    // ENAMETOOLONG
    #[error("file name too long")]
    InvalidFilename = 36,
    // // EDEADLK
    // #[error("resource deadlock avoided")]
    // Deadlock = 37,
    // // ENOSYS
    // #[error("function not implemented")]
    // FunctionNotImplemented = 38,
    // ENOTEMPTY
    #[error("directory not empty")]
    DirectoryNotEmpty = 39,
    #[error("message too long")]
    MessageTooLong = 90,
    #[error("address already in use")]
    AddrInUse = 98,
    #[error("unknown error")]
    Unknown = -1,
}
