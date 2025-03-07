use core::{
    fmt::{self, Write as _},
    ptr, str,
};

#[cfg(feature = "alloc")]
pub use self::os_string::OsString;

#[cfg(feature = "alloc")]
mod os_string;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OsStr {
    inner: [u8],
}

impl OsStr {
    #[must_use]
    pub fn new<S>(s: &S) -> &Self
    where
        S: AsRef<Self> + ?Sized,
    {
        s.as_ref()
    }

    #[must_use]
    pub fn to_str(&self) -> Option<&str> {
        str::from_utf8(&self.inner).ok()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[must_use]
    pub fn from_bytes(slice: &[u8]) -> &Self {
        Self::from_inner(slice)
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.inner
    }

    #[must_use]
    pub fn display(&self) -> Display<'_> {
        Display { os_str: self }
    }

    fn from_inner(inner: &[u8]) -> &Self {
        unsafe { &*(ptr::from_ref(inner) as *const Self) }
    }

    #[cfg(feature = "alloc")]
    fn from_inner_mut(inner: &mut [u8]) -> &mut Self {
        unsafe { &mut *(ptr::from_mut(inner) as *mut Self) }
    }
}

impl AsRef<Self> for OsStr {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl AsRef<OsStr> for str {
    fn as_ref(&self) -> &OsStr {
        OsStr::from_inner(self.as_bytes())
    }
}

impl Default for &OsStr {
    fn default() -> Self {
        OsStr::new("")
    }
}

impl PartialEq<str> for OsStr {
    fn eq(&self, other: &str) -> bool {
        *self == *Self::new(other)
    }
}

impl PartialEq<OsStr> for str {
    fn eq(&self, other: &OsStr) -> bool {
        *other == *OsStr::new(self)
    }
}

impl PartialOrd<str> for OsStr {
    fn partial_cmp(&self, other: &str) -> Option<core::cmp::Ordering> {
        self.partial_cmp(Self::new(other))
    }
}

pub struct Display<'a> {
    os_str: &'a OsStr,
}

impl fmt::Display for Display<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // If we're the empty string then our iterator won't actually yield
        // anything, so perform the formatting manually
        if self.os_str.is_empty() {
            return "".fmt(f);
        }

        for chunk in self.os_str.inner.utf8_chunks() {
            let valid = chunk.valid();
            // If we successfully decoded the whole chunk as a valid string then
            // we can return a direct formatting of the string which will also
            // respect various formatting flags if possible.
            if chunk.invalid().is_empty() {
                return valid.fmt(f);
            }

            f.write_str(valid)?;
            f.write_char(char::REPLACEMENT_CHARACTER)?;
        }
        Ok(())
    }
}
