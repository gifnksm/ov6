use alloc::{
    borrow::{Cow, ToOwned},
    collections::TryReserveError,
    string::String,
};
use core::{
    borrow::Borrow,
    cmp,
    convert::Infallible,
    ops::{Deref, DerefMut},
    str::FromStr,
};

use super::Path;
use crate::os_str::{OsStr, OsString};

#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PathBuf {
    inner: OsString,
}

impl PathBuf {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: OsString::new(),
        }
    }

    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: OsString::with_capacity(capacity),
        }
    }

    #[must_use]
    pub fn as_path(&self) -> &Path {
        Path::new(&self.inner)
    }

    pub fn push<P>(&mut self, path: P)
    where
        P: AsRef<Path>,
    {
        let path = path.as_ref();
        if path.is_absolute() {
            self.inner.clear();
            self.inner.push(path.as_os_str());
            return;
        }
        if !self.inner.as_bytes().ends_with(b"/") {
            self.inner.push("/");
        }
        self.inner.push(path);
    }

    pub fn pop(&mut self) -> bool {
        match self.parent().map(|p| p.as_os_str().as_bytes().len()) {
            Some(len) => {
                self.inner.truncate(len);
                true
            }
            None => false,
        }
    }

    pub fn set_file_name<S>(&mut self, file_name: S)
    where
        S: AsRef<OsStr>,
    {
        if self.file_name().is_some() {
            self.pop();
        }
        self.push(file_name.as_ref())
    }

    pub fn set_extension<S>(&mut self, extension: S) -> bool
    where
        S: AsRef<OsStr>,
    {
        let Some(file_stem) = self.file_stem() else {
            return false;
        };
        let file_stem = file_stem.as_bytes();

        let end_of_file_stem = file_stem[file_stem.len()..].as_ptr().addr();
        let start = self.inner.as_bytes().as_ptr().addr();
        self.inner.truncate(end_of_file_stem.wrapping_sub(start));

        let new = extension.as_ref();
        if !new.is_empty() {
            self.inner.push(".");
            self.inner.push(new);
        }

        true
    }

    pub fn add_extension<S>(&mut self, extension: S) -> bool
    where
        S: AsRef<OsStr>,
    {
        let Some(file_name) = self.file_name() else {
            return false;
        };
        let file_name = file_name.as_bytes();

        let new = extension.as_ref();
        if !new.is_empty() {
            let end_file_name = file_name[file_name.len()..].as_ptr().addr();
            let start = self.inner.as_bytes().as_ptr().addr();
            self.inner.truncate(end_file_name.wrapping_sub(start));

            self.inner.push(".");
            self.inner.push(new);
        }

        true
    }

    #[must_use]
    pub fn as_mut_os_str(&mut self) -> &mut OsStr {
        &mut self.inner
    }

    #[must_use]
    pub fn into_os_string(self) -> OsString {
        self.inner
    }

    #[must_use]
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    pub fn clear(&mut self) {
        self.inner.clear();
    }

    pub fn reserve(&mut self, additional: usize) {
        self.inner.reserve(additional)
    }

    pub fn try_reserve(&mut self, additional: usize) -> Result<(), TryReserveError> {
        self.inner.try_reserve(additional)
    }

    pub fn reserve_exact(&mut self, additional: usize) {
        self.inner.reserve_exact(additional)
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
}

impl Deref for PathBuf {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        Path::new(&self.inner)
    }
}

impl DerefMut for PathBuf {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Path::from_inner_mut(self.as_mut_os_str())
    }
}

impl AsRef<OsStr> for PathBuf {
    fn as_ref(&self) -> &OsStr {
        self.as_os_str()
    }
}

impl AsRef<Path> for PathBuf {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

impl Borrow<Path> for PathBuf {
    fn borrow(&self) -> &Path {
        self
    }
}

impl<P> Extend<P> for PathBuf
where
    P: AsRef<Path>,
{
    fn extend<T>(&mut self, iter: T)
    where
        T: IntoIterator<Item = P>,
    {
        for path in iter {
            self.push(path);
        }
    }
}

impl<'a> From<&'a PathBuf> for Cow<'a, Path> {
    fn from(p: &'a PathBuf) -> Self {
        Cow::Borrowed(p)
    }
}

impl<T> From<&T> for PathBuf
where
    T: ?Sized + AsRef<OsStr>,
{
    fn from(s: &T) -> Self {
        Self::from(s.as_ref().to_os_string())
    }
}

impl<'a> From<Cow<'a, Path>> for PathBuf {
    fn from(cow: Cow<'a, Path>) -> Self {
        cow.into_owned()
    }
}

impl From<OsString> for PathBuf {
    fn from(s: OsString) -> Self {
        Self { inner: s }
    }
}

impl From<PathBuf> for Cow<'_, Path> {
    fn from(p: PathBuf) -> Self {
        Cow::Owned(p)
    }
}

impl From<PathBuf> for OsString {
    fn from(p: PathBuf) -> Self {
        p.into_os_string()
    }
}

impl From<String> for PathBuf {
    fn from(value: String) -> Self {
        Self {
            inner: value.into(),
        }
    }
}

impl<P> FromIterator<P> for PathBuf
where
    P: AsRef<Path>,
{
    fn from_iter<T: IntoIterator<Item = P>>(iter: T) -> Self {
        let mut buf = Self::new();
        buf.extend(iter);
        buf
    }
}

impl FromStr for PathBuf {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from(s))
    }
}

impl<'a> IntoIterator for &'a PathBuf {
    type IntoIter = super::Iter<'a>;
    type Item = &'a OsStr;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl Path {
    #[must_use]
    pub fn to_string_lossy(&self) -> Cow<'_, str> {
        self.inner.to_string_lossy()
    }

    #[must_use]
    pub fn to_path_buf(&self) -> PathBuf {
        PathBuf {
            inner: self.inner.to_os_string(),
        }
    }

    #[must_use]
    pub fn join<P>(&self, path: P) -> PathBuf
    where
        P: AsRef<Self>,
    {
        let mut buf = self.to_path_buf();
        buf.push(path);
        buf
    }

    #[must_use]
    pub fn with_file_name<S>(&self, file_name: S) -> PathBuf
    where
        S: AsRef<OsStr>,
    {
        let mut buf = self.to_path_buf();
        buf.set_file_name(file_name);
        buf
    }

    #[must_use]
    pub fn with_extension<S>(&self, extension: S) -> PathBuf
    where
        S: AsRef<OsStr>,
    {
        let mut buf = self.to_path_buf();
        buf.set_extension(extension);
        buf
    }

    #[must_use]
    pub fn with_added_extension<S>(&self, extension: S) -> PathBuf
    where
        S: AsRef<OsStr>,
    {
        let mut buf = self.to_path_buf();
        buf.add_extension(extension);
        buf
    }
}

impl AsRef<Path> for Cow<'_, OsStr> {
    fn as_ref(&self) -> &Path {
        Path::new(self)
    }
}

impl AsRef<Path> for OsString {
    fn as_ref(&self) -> &Path {
        self.as_os_str().as_ref()
    }
}

impl AsRef<Path> for String {
    fn as_ref(&self) -> &Path {
        self.as_str().as_ref()
    }
}

impl<'a> From<&'a Path> for Cow<'a, Path> {
    fn from(value: &'a Path) -> Self {
        Self::Borrowed(value)
    }
}

impl ToOwned for Path {
    type Owned = PathBuf;

    fn to_owned(&self) -> Self::Owned {
        self.to_path_buf()
    }
}

macro_rules! impl_cmp {
    (<$($life:lifetime),*> $lhs:ty, $rhs: ty) => {
        impl<$($life),*> PartialEq<$rhs> for $lhs {
            #[inline]
            fn eq(&self, other: &$rhs) -> bool {
                <Path as PartialEq>::eq(self, other)
            }
        }

        impl<$($life),*> PartialEq<$lhs> for $rhs {
            #[inline]
            fn eq(&self, other: &$lhs) -> bool {
                <Path as PartialEq>::eq(self, other)
            }
        }

        impl<$($life),*> PartialOrd<$rhs> for $lhs {
            #[inline]
            fn partial_cmp(&self, other: &$rhs) -> Option<cmp::Ordering> {
                <Path as PartialOrd>::partial_cmp(self, other)
            }
        }

        impl<$($life),*> PartialOrd<$lhs> for $rhs {
            #[inline]
            fn partial_cmp(&self, other: &$lhs) -> Option<cmp::Ordering> {
                <Path as PartialOrd>::partial_cmp(self, other)
            }
        }
    };
}

impl_cmp!(<> PathBuf, Path);
impl_cmp!(<'a> PathBuf, &'a Path);
impl_cmp!(<'a> Cow<'a, Path>, Path);
impl_cmp!(<'a, 'b> Cow<'a, Path>, &'b Path);
impl_cmp!(<'a> Cow<'a, Path>, PathBuf);

macro_rules! impl_cmp_os_str {
    (<$($life:lifetime),*> $lhs:ty, $rhs: ty) => {
        impl<$($life),*> PartialEq<$rhs> for $lhs {
            #[inline]
            fn eq(&self, other: &$rhs) -> bool {
                <Path as PartialEq>::eq(self, other.as_ref())
            }
        }

        impl<$($life),*> PartialEq<$lhs> for $rhs {
            #[inline]
            fn eq(&self, other: &$lhs) -> bool {
                <Path as PartialEq>::eq(self.as_ref(), other)
            }
        }

        impl<$($life),*> PartialOrd<$rhs> for $lhs {
            #[inline]
            fn partial_cmp(&self, other: &$rhs) -> Option<cmp::Ordering> {
                <Path as PartialOrd>::partial_cmp(self, other.as_ref())
            }
        }

        impl<$($life),*> PartialOrd<$lhs> for $rhs {
            #[inline]
            fn partial_cmp(&self, other: &$lhs) -> Option<cmp::Ordering> {
                <Path as PartialOrd>::partial_cmp(self.as_ref(), other)
            }
        }
    };
}
impl_cmp_os_str!(<> PathBuf, OsStr);
impl_cmp_os_str!(<'a> PathBuf, &'a OsStr);
impl_cmp_os_str!(<'a> PathBuf, Cow<'a, OsStr>);
impl_cmp_os_str!(<> PathBuf, OsString);
impl_cmp_os_str!(<'a> Path, Cow<'a, OsStr>);
impl_cmp_os_str!(<> Path, OsString);
impl_cmp_os_str!(<'a, 'b> &'a Path, Cow<'b, OsStr>);
impl_cmp_os_str!(<'a> &'a Path, OsString);
impl_cmp_os_str!(<'a> Cow<'a, Path>, OsStr);
impl_cmp_os_str!(<'a, 'b> Cow<'a, Path>, &'b OsStr);
impl_cmp_os_str!(<'a> Cow<'a, Path>, OsString);
