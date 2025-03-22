use alloc::{
    borrow::{Cow, ToOwned},
    collections::TryReserveError,
    string::String,
    vec::Vec,
};
use core::{
    borrow::Borrow,
    cmp,
    convert::Infallible,
    fmt,
    ops::{Deref, DerefMut, Index, IndexMut, RangeFull},
    str::{FromStr, Utf8Error},
};

use super::OsStr;

#[derive(Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OsString {
    inner: Vec<u8>,
}

impl fmt::Debug for OsString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_os_str().fmt(f)
    }
}

impl OsString {
    #[must_use]
    pub const fn new() -> Self {
        Self { inner: Vec::new() }
    }

    #[must_use]
    pub fn from_vec(vec: Vec<u8>) -> Self {
        Self { inner: vec }
    }

    #[must_use]
    pub fn into_vec(self) -> Vec<u8> {
        self.inner
    }

    #[must_use]
    pub fn as_os_str(&self) -> &OsStr {
        OsStr::from_inner(&self.inner)
    }

    pub fn push<T>(&mut self, s: T)
    where
        T: AsRef<OsStr>,
    {
        self.inner.extend_from_slice(&s.as_ref().inner);
    }

    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Vec::with_capacity(capacity),
        }
    }

    pub fn clear(&mut self) {
        self.inner.clear();
    }

    #[must_use]
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    pub fn reserve(&mut self, additional: usize) {
        self.inner.reserve(additional);
    }

    pub fn try_reserve(&mut self, additional: usize) -> Result<(), TryReserveError> {
        self.inner.try_reserve(additional)
    }

    pub fn reserve_exact(&mut self, additional: usize) {
        self.inner.reserve_exact(additional);
    }

    pub fn try_reserve_exact(&mut self, additional: usize) -> Result<(), TryReserveError> {
        self.inner.try_reserve_exact(additional)
    }

    pub fn shrink_to_fit(&mut self) {
        self.inner.shrink_to_fit();
    }

    pub fn shrink_to(&mut self, min_capacity: usize) {
        self.inner.shrink_to(min_capacity);
    }

    pub fn truncate(&mut self, len: usize) {
        self.inner.truncate(len);
    }
}

impl Deref for OsString {
    type Target = OsStr;

    fn deref(&self) -> &Self::Target {
        &self[..]
    }
}

impl DerefMut for OsString {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self[..]
    }
}

impl AsRef<OsStr> for OsString {
    fn as_ref(&self) -> &OsStr {
        self.as_os_str()
    }
}

impl Borrow<OsStr> for OsString {
    fn borrow(&self) -> &OsStr {
        self.as_os_str()
    }
}

impl<'a> Extend<&'a OsStr> for OsString {
    fn extend<T: IntoIterator<Item = &'a OsStr>>(&mut self, iter: T) {
        for s in iter {
            self.push(s);
        }
    }
}

impl<'a> Extend<Cow<'a, OsStr>> for OsString {
    fn extend<T: IntoIterator<Item = Cow<'a, OsStr>>>(&mut self, iter: T) {
        for s in iter {
            self.push(&s);
        }
    }
}

impl Extend<Self> for OsString {
    fn extend<T: IntoIterator<Item = Self>>(&mut self, iter: T) {
        for s in iter {
            self.push(&s);
        }
    }
}

impl<'a> From<&'a OsString> for Cow<'a, OsStr> {
    fn from(value: &'a OsString) -> Self {
        Self::Borrowed(value)
    }
}

impl<T> From<&T> for OsString
where
    T: ?Sized + AsRef<OsStr>,
{
    fn from(s: &T) -> Self {
        s.as_ref().to_os_string()
    }
}

impl<'a> From<Cow<'a, OsStr>> for OsString {
    fn from(s: Cow<'a, OsStr>) -> Self {
        s.into_owned()
    }
}

impl From<OsString> for Cow<'_, OsStr> {
    fn from(s: OsString) -> Self {
        Cow::Owned(s)
    }
}

impl From<String> for OsString {
    fn from(s: String) -> Self {
        Self {
            inner: s.into_bytes(),
        }
    }
}

impl<'a> FromIterator<&'a OsStr> for OsString {
    fn from_iter<T: IntoIterator<Item = &'a OsStr>>(iter: T) -> Self {
        let mut buf = Self::new();
        buf.extend(iter);
        buf
    }
}

impl<'a> FromIterator<Cow<'a, OsStr>> for OsString {
    fn from_iter<T: IntoIterator<Item = Cow<'a, OsStr>>>(iter: T) -> Self {
        let mut buf = Self::new();
        buf.extend(iter);
        buf
    }
}

impl FromIterator<Self> for OsString {
    fn from_iter<T: IntoIterator<Item = Self>>(iter: T) -> Self {
        let mut buf = Self::new();
        buf.extend(iter);
        buf
    }
}

impl FromStr for OsString {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from(s))
    }
}

impl Index<RangeFull> for OsString {
    type Output = OsStr;

    fn index(&self, _: RangeFull) -> &Self::Output {
        OsStr::from_inner(&self.inner)
    }
}

impl IndexMut<RangeFull> for OsString {
    fn index_mut(&mut self, _: RangeFull) -> &mut Self::Output {
        OsStr::from_inner_mut(&mut self.inner)
    }
}

impl PartialEq<str> for OsString {
    fn eq(&self, other: &str) -> bool {
        *self == *Self::from(other)
    }
}

impl PartialOrd<str> for OsString {
    fn partial_cmp(&self, other: &str) -> Option<cmp::Ordering> {
        self.as_os_str().partial_cmp(Self::from(other).as_os_str())
    }
}

impl OsStr {
    #[must_use]
    pub fn to_string_lossy(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.inner)
    }

    #[must_use]
    pub fn to_os_string(&self) -> OsString {
        OsString {
            inner: self.inner.to_owned(),
        }
    }
}

impl AsRef<OsStr> for String {
    fn as_ref(&self) -> &OsStr {
        self.as_str().as_ref()
    }
}

impl<'a> From<&'a OsStr> for Cow<'a, OsStr> {
    fn from(value: &'a OsStr) -> Self {
        Self::Borrowed(value)
    }
}

macro_rules! impl_cmp {
    ($lhs:ty, $rhs:ty) => {
        impl<'a, 'b> PartialEq<$rhs> for $lhs {
            #[inline]
            fn eq(&self, other: &$rhs) -> bool {
                <OsStr as PartialEq>::eq(self, other)
            }
        }

        impl<'a, 'b> PartialEq<$lhs> for $rhs {
            #[inline]
            fn eq(&self, other: &$lhs) -> bool {
                <OsStr as PartialEq>::eq(self, other)
            }
        }

        impl<'a, 'b> PartialOrd<$rhs> for $lhs {
            #[inline]
            fn partial_cmp(&self, other: &$rhs) -> Option<cmp::Ordering> {
                <OsStr as PartialOrd>::partial_cmp(self, other)
            }
        }

        impl<'a, 'b> PartialOrd<$lhs> for $rhs {
            #[inline]
            fn partial_cmp(&self, other: &$lhs) -> Option<cmp::Ordering> {
                <OsStr as PartialOrd>::partial_cmp(self, other)
            }
        }
    };
}

impl_cmp!(OsString, OsStr);
impl_cmp!(OsString, &'a OsStr);
impl_cmp!(Cow<'a, OsStr>, OsStr);
impl_cmp!(Cow<'a, OsStr>, &'b OsStr);
impl_cmp!(Cow<'a, OsStr>, OsString);

impl ToOwned for OsStr {
    type Owned = OsString;

    fn to_owned(&self) -> Self::Owned {
        self.to_os_string()
    }
}

impl<'a> TryFrom<&'a OsStr> for &'a str {
    type Error = Utf8Error;

    fn try_from(value: &'a OsStr) -> Result<Self, Self::Error> {
        str::from_utf8(&value.inner)
    }
}
