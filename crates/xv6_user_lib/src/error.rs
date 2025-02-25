#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("stream did not contain valid UTF-8")]
    InvalidUtf8,
    #[error("failed to fill whole buffer")]
    ReadExactEof,
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

impl From<xv6_syscall::Error> for Error {
    fn from(value: xv6_syscall::Error) -> Self {
        match value {
            xv6_syscall::Error::Unknown => Error::Unknown,
        }
    }
}
