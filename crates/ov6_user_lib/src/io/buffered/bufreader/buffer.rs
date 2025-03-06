use core::{cmp, io::BorrowedBuf, mem::MaybeUninit};

use alloc_crate::boxed::Box;

use crate::io::Read;

pub(super) struct Buffer {
    buf: Box<[MaybeUninit<u8>]>,
    pos: usize,
    filled: usize,
    initialized: usize,
}

impl Buffer {
    pub(super) fn with_capacity(capacity: usize) -> Self {
        let buf = Box::new_uninit_slice(capacity);
        Self {
            buf,
            pos: 0,
            filled: 0,
            initialized: 0,
        }
    }

    pub(super) fn buffer(&self) -> &[u8] {
        unsafe {
            self.buf
                .get_unchecked(self.pos..self.filled)
                .assume_init_ref()
        }
    }

    pub(crate) fn capacity(&self) -> usize {
        self.buf.len()
    }

    pub(crate) fn filled(&self) -> usize {
        self.filled
    }

    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    pub(crate) fn discard_buffer(&mut self) {
        self.pos = 0;
        self.filled = 0;
    }

    pub(crate) fn consume(&mut self, amt: usize) {
        self.pos = cmp::min(self.pos + amt, self.filled);
    }

    pub(crate) fn fill_buf(
        &mut self,
        mut reader: impl Read,
    ) -> Result<&[u8], crate::error::Ov6Error> {
        if self.pos >= self.filled {
            debug_assert!(self.pos == self.filled);

            let mut buf = BorrowedBuf::from(&mut *self.buf);
            unsafe {
                buf.set_init(self.initialized);
            }
            let result = reader.read_buf(buf.unfilled());

            self.pos = 0;
            self.filled = buf.len();
            self.initialized = buf.init_len();

            result?;
        }
        Ok(self.buffer())
    }
}
