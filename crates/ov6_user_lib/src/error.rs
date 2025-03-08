use ov6_syscall::SyscallError;

#[derive(Debug, thiserror::Error)]
pub enum Ov6Error {
    #[error("stream did not contain valid UTF-8")]
    InvalidUtf8,
    #[error("failed to fill whole buffer")]
    ReadExactEof,
    #[error("failed to write whole buffer")]
    WriteAllEof,
    #[error("not a directory")]
    NotADirectory,
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
    fn from(value: SyscallError) -> Self {
        match value {
            SyscallError::Unknown => Self::Unknown,
        }
    }
}
