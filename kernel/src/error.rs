use ov6_syscall::SyscallError;

#[derive(Debug, thiserror::Error)]
pub enum KernelError {
    #[error("unknown error")]
    Unknown,
}

impl From<KernelError> for SyscallError {
    fn from(error: KernelError) -> Self {
        match error {
            KernelError::Unknown => Self::Unknown,
        }
    }
}
