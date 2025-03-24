use core::{error::Error, fmt};

pub use self::{bufreader::BufReader, bufwriter::BufWriter, linewriter::LineWriter};
use crate::error::Ov6Error;

mod bufreader;
mod bufwriter;
mod linewriter;
mod linewritershim;

#[derive(Debug)]
pub struct IntoInnerError<W>(W, Ov6Error);

impl<W> IntoInnerError<W> {
    fn new(writer: W, error: Ov6Error) -> Self {
        Self(writer, error)
    }

    fn new_wrapped<W2>(self, f: impl FnOnce(W) -> W2) -> IntoInnerError<W2> {
        let Self(writer, error) = self;
        IntoInnerError::new(f(writer), error)
    }

    pub fn error(&self) -> &Ov6Error {
        &self.1
    }

    pub fn into_inner(self) -> W {
        self.0
    }

    pub fn into_error(self) -> Ov6Error {
        self.1
    }

    pub fn into_parts(self) -> (Ov6Error, W) {
        (self.1, self.0)
    }
}

impl<W> From<IntoInnerError<W>> for Ov6Error {
    fn from(e: IntoInnerError<W>) -> Self {
        e.1
    }
}

impl<W: Send + fmt::Debug> Error for IntoInnerError<W> {}

impl<W> fmt::Display for IntoInnerError<W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error().fmt(f)
    }
}
