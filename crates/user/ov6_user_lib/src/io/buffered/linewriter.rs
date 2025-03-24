use core::fmt;

use super::{BufWriter, IntoInnerError, linewritershim::LineWriterShim};
use crate::io::Write;

pub struct LineWriter<W>
where
    W: ?Sized + Write,
{
    inner: BufWriter<W>,
}

impl<W> LineWriter<W>
where
    W: Write,
{
    pub fn new(inner: W) -> Self {
        Self {
            inner: BufWriter::with_capacity(1024, inner),
        }
    }

    pub fn with_capacity(capacity: usize, inner: W) -> Self {
        Self {
            inner: BufWriter::with_capacity(capacity, inner),
        }
    }

    pub fn get_mut(&mut self) -> &mut W {
        self.inner.get_mut()
    }

    pub fn into_inner(self) -> Result<W, IntoInnerError<Self>> {
        self.inner
            .into_inner()
            .map_err(|err| err.new_wrapped(|inner| Self { inner }))
    }
}

impl<W> LineWriter<W>
where
    W: ?Sized + Write,
{
    pub fn get_ref(&self) -> &W {
        self.inner.get_ref()
    }
}

impl<W> Write for LineWriter<W>
where
    W: ?Sized + Write,
{
    fn write(&mut self, buf: &[u8]) -> Result<usize, crate::error::Ov6Error> {
        LineWriterShim::new(&mut self.inner).write(buf)
    }

    fn flush(&mut self) -> Result<(), crate::error::Ov6Error> {
        self.inner.flush()
    }

    fn write_all(&mut self, buf: &[u8]) -> Result<(), crate::error::Ov6Error> {
        LineWriterShim::new(&mut self.inner).write_all(buf)
    }
}

impl<W> fmt::Debug for LineWriter<W>
where
    W: ?Sized + Write + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LineWriter")
            .field("writer", &self.get_ref())
            .field(
                "buffer",
                &format_args!("{}/{}", self.inner.buffer().len(), self.inner.capacity()),
            )
            .finish()
    }
}
