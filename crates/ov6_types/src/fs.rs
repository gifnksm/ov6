use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct RawFd(usize);

impl fmt::Display for RawFd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl From<RawFd> for usize {
    fn from(value: RawFd) -> Self {
        value.0
    }
}

impl From<usize> for RawFd {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl RawFd {
    #[must_use]
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}
