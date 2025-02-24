#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not a directory")]
    NotADirectory,
    #[error("unknown error")]
    Unknown,
}
