#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not a directory")]
    NotADirectory,
    #[error("unknown error")]
    Unknown,
}

impl From<xv6_syscall::Error> for Error {
    fn from(value: xv6_syscall::Error) -> Self {
        match value {
            xv6_syscall::Error::Unknown => Error::Unknown,
        }
    }
}
