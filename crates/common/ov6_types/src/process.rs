use core::{fmt, num::NonZero, str::FromStr};

use dataview::Pod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct ProcId(NonZero<u32>);

unsafe impl Pod for ProcId {}

impl fmt::Display for ProcId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl From<ProcId> for u32 {
    fn from(value: ProcId) -> Self {
        value.0.get()
    }
}

impl From<ProcId> for NonZero<u32> {
    fn from(value: ProcId) -> Self {
        value.0
    }
}

impl From<NonZero<u32>> for ProcId {
    fn from(value: NonZero<u32>) -> Self {
        Self(value)
    }
}

impl ProcId {
    #[must_use]
    pub const fn new(value: NonZero<u32>) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> NonZero<u32> {
        self.0
    }
}

impl FromStr for ProcId {
    type Err = <NonZero<u32> as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(Self::new)
    }
}
