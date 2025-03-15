use self::buffer::Buffer;
use crate::{
    error::Ov6Error,
    io::{BufRead, DEFAULT_BUF_SIZE, Read},
};

mod buffer;

pub struct BufReader<R: ?Sized> {
    buf: Buffer,
    inner: R,
}

impl<R> BufReader<R>
where
    R: Read,
{
    pub fn new(inner: R) -> Self {
        Self::with_capacity(DEFAULT_BUF_SIZE, inner)
    }

    pub fn with_capacity(capacity: usize, inner: R) -> Self {
        Self {
            inner,
            buf: Buffer::with_capacity(capacity),
        }
    }

    pub fn capacity(&self) -> usize {
        self.buf.capacity()
    }

    fn discard_buffer(&mut self) {
        self.buf.discard_buffer();
    }
}

impl<R> Read for BufReader<R>
where
    R: Read,
{
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Ov6Error> {
        if self.buf.pos() == self.buf.filled() && buf.len() >= self.capacity() {
            self.discard_buffer();
            return self.inner.read(buf);
        }
        let mut rem = self.fill_buf()?;
        let nread = rem.read(buf)?;
        self.consume(nread);
        Ok(nread)
    }
}

impl<R> BufRead for BufReader<R>
where
    R: Read,
{
    fn fill_buf(&mut self) -> Result<&[u8], Ov6Error> {
        self.buf.fill_buf(&mut self.inner)
    }

    fn consume(&mut self, amt: usize) {
        self.buf.consume(amt);
    }
}
