#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unknown error")]
    Unknown,
}

impl From<Error> for xv6_syscall::Error {
    fn from(error: Error) -> Self {
        match error {
            Error::Unknown => xv6_syscall::Error::Unknown,
        }
    }
}
