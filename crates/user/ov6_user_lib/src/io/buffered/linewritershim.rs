use super::BufWriter;
use crate::{error::Ov6Error, io::Write};

#[derive(Debug)]
pub struct LineWriterShim<'a, W>
where
    W: ?Sized + Write,
{
    buffer: &'a mut BufWriter<W>,
}

impl<'a, W> LineWriterShim<'a, W>
where
    W: ?Sized + Write,
{
    pub fn new(buffer: &'a mut BufWriter<W>) -> Self {
        Self { buffer }
    }

    // fn inner(&self) -> &W {
    //     self.buffer.get_ref()
    // }

    fn inner_mut(&mut self) -> &mut W {
        self.buffer.get_mut()
    }

    pub fn buffered(&self) -> &[u8] {
        self.buffer.buffer()
    }

    fn flush_if_completed_line(&mut self) -> Result<(), Ov6Error> {
        match self.buffered().last().copied() {
            Some(b'\n') => self.buffer.flush_buf(),
            _ => Ok(()),
        }
    }
}

impl<W: ?Sized + Write> Write for LineWriterShim<'_, W> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Ov6Error> {
        let newline_idx = match memchr::memchr(b'\n', buf) {
            None => {
                self.flush_if_completed_line()?;
                return self.buffer.write(buf);
            }
            Some(newline_idx) => newline_idx + 1,
        };

        self.buffer.flush_buf()?;

        let lines = &buf[..newline_idx];

        let flushed = self.inner_mut().write(lines)?;

        if flushed == 0 {
            return Ok(0);
        }

        let tail = if flushed >= newline_idx {
            let tail = &buf[flushed..];
            if tail.len() >= self.buffer.capacity() {
                return Ok(flushed);
            }
            tail
        } else if newline_idx - flushed <= self.buffer.capacity() {
            &buf[flushed..newline_idx]
        } else {
            let scan_area = &buf[flushed..];
            let scan_area = &scan_area[..self.buffer.capacity()];
            memchr::memchr(b'\n', scan_area)
                .map_or(scan_area, |newline_idx| &scan_area[..=newline_idx])
        };

        let buffered = self.buffer.write_to_buf(tail);
        Ok(flushed + buffered)
    }

    fn flush(&mut self) -> Result<(), Ov6Error> {
        self.buffer.flush()
    }

    fn write_all(&mut self, buf: &[u8]) -> Result<(), Ov6Error> {
        match memchr::memchr(b'\n', buf) {
            None => {
                self.flush_if_completed_line()?;
                self.buffer.write_all(buf)
            }
            Some(newline_idx) => {
                let (lines, tail) = buf.split_at(newline_idx + 1);
                if self.buffered().is_empty() {
                    self.inner_mut().write_all(lines)?;
                } else {
                    self.buffer.write_all(lines)?;
                    self.buffer.flush_buf()?;
                }

                self.buffer.write_all(tail)
            }
        }
    }
}
