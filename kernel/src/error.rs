#[derive(Debug, thiserror::Error)]
pub enum KernelError {
    #[error("unknown error")]
    Unknown,
}

impl From<KernelError> for ov6_syscall::Error {
    fn from(error: KernelError) -> Self {
        match error {
            KernelError::Unknown => Self::Unknown,
        }
    }
}
