use ov6_syscall::{RegisterDecodeError, SyscallError};

#[derive(Debug, thiserror::Error)]
pub enum KernelError {
    #[error("sycall decode: {0}")]
    SyscallDecode(RegisterDecodeError),
    #[error("unknown error")]
    Unknown,
}

impl From<KernelError> for SyscallError {
    fn from(error: KernelError) -> Self {
        match error {
            KernelError::SyscallDecode(_) | KernelError::Unknown => Self::Unknown,
        }
    }
}
