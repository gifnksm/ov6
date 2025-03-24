// some codes are borrowed from Rust standard library

use core::{cmp, io::BorrowedCursor};

use alloc_crate::{string::String, vec::Vec};

pub use self::{buffered::*, stdio::*};
use crate::error::Ov6Error;

const DEFAULT_BUF_SIZE: usize = 1024;

mod buffered;
mod stdio;

pub(crate) fn cleanup() {
    stdio::cleanup();
}

pub trait Read {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Ov6Error>;

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> Result<usize, Ov6Error> {
        let start_len = buf.len();

        loop {
            let mut read_buf = [0; 256];
            let n = match self.read(&mut read_buf) {
                Ok(n) => n,
                Err(e) if e.is_interrupted() => continue,
                Err(e) => return Err(e),
            };
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&read_buf[..n]);
        }

        Ok(buf.len() - start_len)
    }

    fn read_to_string(&mut self, buf: &mut String) -> Result<usize, Ov6Error> {
        unsafe { append_to_string(buf, |b| self.read_to_end(b)) }
    }

    fn read_exact(&mut self, mut buf: &mut [u8]) -> Result<(), Ov6Error> {
        while !buf.is_empty() {
            let n = match self.read(buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.is_interrupted() => continue,
                Err(e) => return Err(e),
            };
            buf = &mut buf[n..];
        }

        if !buf.is_empty() {
            return Err(Ov6Error::ReadExactEof);
        }

        Ok(())
    }

    fn read_buf(&mut self, mut cursor: BorrowedCursor<'_>) -> Result<(), Ov6Error> {
        let n = self.read(cursor.ensure_init().init_mut())?;
        cursor.advance(n);
        Ok(())
    }
}

struct Guard<'a> {
    buf: &'a mut Vec<u8>,
    len: usize,
}

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        unsafe {
            self.buf.set_len(self.len);
        }
    }
}

unsafe fn append_to_string<F>(buf: &mut String, f: F) -> Result<usize, Ov6Error>
where
    F: FnOnce(&mut Vec<u8>) -> Result<usize, Ov6Error>,
{
    let mut g = Guard {
        len: buf.len(),
        buf: unsafe { buf.as_mut_vec() },
    };
    let ret = f(g.buf);
    let appended = unsafe { g.buf.get_unchecked(g.len..) };
    if str::from_utf8(appended).is_err() {
        ret.and(Err(Ov6Error::InvalidUtf8))
    } else {
        g.len = g.buf.len();
        ret
    }
}

pub trait Write {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Ov6Error>;
    fn flush(&mut self) -> Result<(), Ov6Error>;

    fn write_all(&mut self, mut buf: &[u8]) -> Result<(), Ov6Error> {
        while !buf.is_empty() {
            let n = match self.write(buf) {
                Ok(0) => return Err(Ov6Error::WriteAllEof),
                Ok(n) => n,
                Err(e) if e.is_interrupted() => continue,
                Err(e) => return Err(e),
            };
            buf = &buf[n..];
        }
        Ok(())
    }

    fn by_ref(&mut self) -> &mut Self
    where
        Self: Sized,
    {
        self
    }
}

pub trait BufRead: Read {
    fn fill_buf(&mut self) -> Result<&[u8], Ov6Error>;
    fn consume(&mut self, amt: usize);

    fn has_data_left(&mut self) -> Result<bool, Ov6Error> {
        self.fill_buf().map(|b| !b.is_empty())
    }

    fn read_until(&mut self, byte: u8, buf: &mut Vec<u8>) -> Result<usize, Ov6Error> {
        let mut read = 0;
        loop {
            let (done, used) = {
                let available = match self.fill_buf() {
                    Ok(n) => n,
                    Err(e) if e.is_interrupted() => continue,
                    Err(e) => return Err(e),
                };
                if let Some(i) = memchr::memchr(byte, available) {
                    buf.extend_from_slice(&available[..=i]);
                    (true, i + 1)
                } else {
                    buf.extend_from_slice(available);
                    (false, available.len())
                }
            };
            self.consume(used);
            read += used;
            if done || used == 0 {
                return Ok(read);
            }
        }
    }

    fn skip_until(&mut self, byte: u8) -> Result<usize, Ov6Error> {
        let mut read = 0;
        loop {
            let (done, used) = {
                let available = match self.fill_buf() {
                    Ok(n) => n,
                    Err(e) if e.is_interrupted() => continue,
                    Err(e) => return Err(e),
                };
                memchr::memchr(byte, available).map_or((false, available.len()), |i| (true, i + 1))
            };
            self.consume(used);
            read += used;
            if done || used == 0 {
                return Ok(read);
            }
        }
    }

    fn read_line(&mut self, buf: &mut String) -> Result<usize, Ov6Error> {
        unsafe { append_to_string(buf, |b| self.read_until(b'\n', b)) }
    }
}

impl<R> Read for &mut R
where
    R: Read,
{
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Ov6Error> {
        (**self).read(buf)
    }
}

impl Read for &[u8] {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Ov6Error> {
        let amt = cmp::min(buf.len(), self.len());
        let (a, b) = self.split_at(amt);
        buf[..amt].copy_from_slice(a);
        *self = b;
        Ok(amt)
    }
}
