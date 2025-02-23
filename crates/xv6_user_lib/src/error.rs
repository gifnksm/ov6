use core::fmt;

#[derive(Debug)]
pub enum Error {
    NotADirectory,
    Unknown,
}

impl core::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotADirectory => write!(f, "not a directory"),
            Self::Unknown => write!(f, "unknown error"),
        }
    }
}
