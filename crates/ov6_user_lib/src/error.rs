#[derive(Debug, thiserror::Error)]
pub enum Error {
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
impl Error {
    pub fn is_interrupted(&self) -> bool {
        false // TODO
    }
}

impl From<ov6_syscall::Error> for Error {
    fn from(value: ov6_syscall::Error) -> Self {
        match value {
            ov6_syscall::Error::Unknown => Error::Unknown,
        }
    }
}
