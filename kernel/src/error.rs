#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unknown error")]
    Unknown,
}

impl From<Error> for ov6_syscall::Error {
    fn from(error: Error) -> Self {
        match error {
            Error::Unknown => ov6_syscall::Error::Unknown,
        }
    }
}
