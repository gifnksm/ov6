use core::{cmp, error::Error, fmt, iter::FusedIterator, ptr};

#[cfg(feature = "alloc")]
pub use self::path_buf::PathBuf;
use crate::os_str::{self, OsStr};

#[cfg(feature = "alloc")]
mod path_buf;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Path {
    inner: OsStr,
}

impl Path {
    #[must_use]
    pub fn new<S>(s: &S) -> &Self
    where
        S: AsRef<OsStr> + ?Sized,
    {
        Self::from_inner(s.as_ref())
    }

    #[must_use]
    pub fn as_os_str(&self) -> &OsStr {
        &self.inner
    }

    #[must_use]
    pub fn as_mut_os_str(&mut self) -> &mut OsStr {
        &mut self.inner
    }

    #[must_use]
    pub fn to_str(&self) -> Option<&str> {
        self.inner.to_str()
    }

    #[must_use]
    pub fn is_absolute(&self) -> bool {
        self.inner.as_bytes().starts_with(b"/")
    }

    #[must_use]
    pub fn is_relative(&self) -> bool {
        !self.is_absolute()
    }

    #[must_use]
    pub fn parent(&self) -> Option<&Self> {
        let mut comps = self.components();
        match comps.next_back()? {
            Component::CurDir | Component::ParentDir | Component::Normal(_) => {
                Some(comps.as_path())
            }
            Component::RootDir => None,
        }
    }

    pub fn ancestors(&self) -> Ancestors<'_> {
        Ancestors { next: Some(self) }
    }

    #[must_use]
    pub fn file_name(&self) -> Option<&OsStr> {
        match self.components().next_back()? {
            Component::Normal(p) => Some(p),
            _ => None,
        }
    }

    pub fn strip_prefix<P>(&self, base: P) -> Result<&Self, StripPrefixError>
    where
        P: AsRef<Self>,
    {
        let base = base.as_ref();
        iter_after(self.components(), base.components())
            .map(|c| c.as_path())
            .ok_or(StripPrefixError(()))
    }

    pub fn starts_with<P>(&self, base: P) -> bool
    where
        P: AsRef<Self>,
    {
        let base = base.as_ref();
        iter_after(self.components(), base.components()).is_some()
    }

    pub fn ends_with<P>(&self, child: P) -> bool
    where
        P: AsRef<Self>,
    {
        let child = child.as_ref();
        iter_after(self.components().rev(), child.components().rev()).is_some()
    }

    #[must_use]
    pub fn file_stem(&self) -> Option<&OsStr> {
        let (before, after) = self.file_name().map(rsplit_file_at_dot)?;
        before.or(after)
    }

    #[must_use]
    pub fn file_prefix(&self) -> Option<&OsStr> {
        self.file_name()
            .map(split_file_at_dot)
            .map(|(before, _after)| before)
    }

    #[must_use]
    pub fn extension(&self) -> Option<&OsStr> {
        let (before, after) = self.file_name().map(rsplit_file_at_dot)?;
        before.and(after)
    }

    pub fn components(&self) -> Components<'_> {
        Components::new(self)
    }

    pub fn iter(&self) -> Iter<'_> {
        Iter {
            inner: self.components(),
        }
    }

    #[must_use]
    pub fn display(&self) -> Display<'_> {
        Display {
            inner: self.inner.display(),
        }
    }

    fn from_inner(inner: &OsStr) -> &Self {
        unsafe { &*(ptr::from_ref(inner) as *const Self) }
    }

    #[cfg(feature = "alloc")]
    fn from_inner_mut(inner: &mut OsStr) -> &mut Self {
        unsafe { &mut *(ptr::from_mut(inner) as *mut Self) }
    }
}

pub struct Display<'a> {
    inner: os_str::Display<'a>,
}

impl fmt::Display for Display<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.inner, f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripPrefixError(());

impl fmt::Display for StripPrefixError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("prefix not found")
    }
}

impl Error for StripPrefixError {}

impl AsRef<OsStr> for Path {
    fn as_ref(&self) -> &OsStr {
        &self.inner
    }
}

impl AsRef<Path> for OsStr {
    fn as_ref(&self) -> &Path {
        Path::new(self)
    }
}

impl AsRef<Self> for Path {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl AsRef<Path> for str {
    fn as_ref(&self) -> &Path {
        Path::new(self)
    }
}

impl<'a> IntoIterator for &'a Path {
    type IntoIter = Iter<'a>;
    type Item = &'a OsStr;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

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

impl_cmp_os_str!(<> Path, OsStr);
impl_cmp_os_str!(<'a> Path, &'a OsStr);
impl_cmp_os_str!(<'a> &'a Path, OsStr);

#[derive(Debug, Clone)]
#[must_use]
pub struct Ancestors<'a> {
    next: Option<&'a Path>,
}

impl<'a> Iterator for Ancestors<'a> {
    type Item = &'a Path;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.next;
        self.next = next.and_then(Path::parent);
        next
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Component<'a> {
    RootDir,
    CurDir,
    ParentDir,
    Normal(&'a OsStr),
}

impl<'a> Component<'a> {
    #[must_use]
    pub fn as_os_str(self) -> &'a OsStr {
        match self {
            Component::RootDir => OsStr::from_bytes(b"/"),
            Component::CurDir => OsStr::from_bytes(b"."),
            Component::ParentDir => OsStr::from_bytes(b".."),
            Component::Normal(path) => path,
        }
    }
}

impl AsRef<OsStr> for Component<'_> {
    fn as_ref(&self) -> &OsStr {
        self.as_os_str()
    }
}

impl AsRef<Path> for Component<'_> {
    fn as_ref(&self) -> &Path {
        self.as_os_str().as_ref()
    }
}

#[must_use]
#[derive(Debug, Clone)]
pub struct Components<'a> {
    path: &'a [u8],
    has_root_dir: bool,
}

impl<'a> Components<'a> {
    #[must_use]
    pub fn as_path(&self) -> &'a Path {
        Path::new(OsStr::from_bytes(self.path))
    }

    fn new(path: &'a Path) -> Self {
        let path = path.as_os_str().as_bytes();
        let mut this = Self {
            path,
            has_root_dir: path.starts_with(b"/"),
        };
        this.trim_leading_slashes();
        this.trim_trailing_slashes();
        this
    }

    fn trim_leading_slashes(&mut self) {
        if self.has_root_dir {
            return;
        }

        let mut path = self.path;
        while let Some(rest) = path.strip_prefix(b"/") {
            path = rest;
        }
        self.path = path;
    }

    fn trim_trailing_slashes(&mut self) {
        while !self.has_root_dir || self.path.len() > 1 {
            // do not trim root slash
            let Some(path) = self.path.strip_suffix(b"/") else {
                break;
            };
            self.path = path;
        }
    }
}

impl<'a> Iterator for Components<'a> {
    type Item = Component<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.path.is_empty() {
            return None;
        }

        if self.has_root_dir && self.path.starts_with(b"/") {
            self.has_root_dir = false;
            self.trim_leading_slashes();
            return Some(Component::RootDir);
        }

        let (comp, rest) = self
            .path
            .split_once(|b| *b == b'/')
            .unwrap_or((self.path, &[]));
        self.path = rest;
        self.trim_leading_slashes();
        match comp {
            b".." => Some(Component::ParentDir),
            b"." => Some(Component::CurDir),
            b"" => None,
            comp => Some(Component::Normal(OsStr::from_bytes(comp))),
        }
    }
}

impl DoubleEndedIterator for Components<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.has_root_dir && self.path == b"/" {
            self.has_root_dir = false;
            self.path = &[];
            return Some(Component::RootDir);
        }

        if self.path.is_empty() {
            return None;
        }

        let (rest, comp) = self
            .path
            .rsplit_once(|b| *b == b'/')
            .unwrap_or((&[], self.path));
        if rest.is_empty() && self.has_root_dir {
            assert_eq!(self.path[0], b'/');
            self.path = &self.path[..1];
        } else {
            self.path = rest;
        }
        self.trim_trailing_slashes();
        match comp {
            b".." => Some(Component::ParentDir),
            b"." => Some(Component::CurDir),
            b"" => None,
            comp => Some(Component::Normal(OsStr::from_bytes(comp))),
        }
    }
}

impl FusedIterator for Components<'_> {}

impl AsRef<OsStr> for Components<'_> {
    fn as_ref(&self) -> &OsStr {
        self.as_path().as_os_str()
    }
}

impl AsRef<Path> for Components<'_> {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

#[must_use]
#[derive(Debug, Clone)]
pub struct Iter<'a> {
    inner: Components<'a>,
}

impl Iter<'_> {
    #[must_use]
    pub fn as_path(&self) -> &Path {
        self.inner.as_path()
    }
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a OsStr;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(Component::as_os_str)
    }
}

impl DoubleEndedIterator for Iter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back().map(Component::as_os_str)
    }
}

impl FusedIterator for Iter<'_> {}

impl AsRef<OsStr> for Iter<'_> {
    fn as_ref(&self) -> &OsStr {
        self.as_path().as_os_str()
    }
}

impl AsRef<Path> for Iter<'_> {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

fn iter_after<'a, 'b, I, J>(mut iter: I, mut prefix: J) -> Option<I>
where
    I: Iterator<Item = Component<'a>> + Clone,
    J: Iterator<Item = Component<'b>>,
{
    loop {
        let mut iter_next = iter.clone();
        match (&iter_next.next(), &prefix.next()) {
            (Some(x), Some(y)) if x == y => {}
            (_, None) => return Some(iter),
            (_, Some(_)) => return None,
        }
        iter = iter_next;
    }
}

fn rsplit_file_at_dot(file: &OsStr) -> (Option<&OsStr>, Option<&OsStr>) {
    if file.as_bytes() == b".." {
        return (Some(file), None);
    }

    let mut iter = file.as_bytes().rsplitn(2, |b| *b == b'.');
    let after = iter.next();
    let before = iter.next();
    if before == Some(b"") {
        (Some(file), None)
    } else {
        (before.map(OsStr::from_bytes), after.map(OsStr::from_bytes))
    }
}

fn split_file_at_dot(file: &OsStr) -> (&OsStr, Option<&OsStr>) {
    let slice = file.as_bytes();
    if slice == b".." {
        return (file, None);
    }

    let i = match slice[1..].iter().position(|b| *b == b'.') {
        Some(i) => i + 1,
        None => return (file, None),
    };
    let before = &slice[..i];
    let after = &slice[i + 1..];
    (OsStr::from_bytes(before), Some(OsStr::from_bytes(after)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_new() {
        let os_str = OsStr::new("/home/user");
        let path = Path::new(&os_str);
        assert_eq!(path.as_os_str(), os_str);
    }

    #[test]
    fn test_path_is_absolute() {
        let path = Path::new("/home/user");
        assert!(path.is_absolute());
        assert!(!path.is_relative());
    }

    #[test]
    fn test_path_is_relative() {
        let path = Path::new("home/user");
        assert!(path.is_relative());
        assert!(!path.is_absolute());
    }

    #[test]
    fn test_path_to_str() {
        let path = Path::new("/home/user");
        assert_eq!(path.to_str(), Some("/home/user"));
    }

    #[test]
    fn test_components() {
        let path = Path::new("/home/user");
        let mut cs = path.components();
        assert_eq!(cs.as_path(), Path::new("/home/user"));
        assert_eq!(cs.next(), Some(Component::RootDir));
        assert_eq!(cs.as_path(), Path::new("home/user"));
        assert_eq!(cs.next(), Some(Component::Normal(OsStr::new("home"))));
        assert_eq!(cs.as_path(), Path::new("user"));
        assert_eq!(cs.next(), Some(Component::Normal(OsStr::new("user"))));
        assert_eq!(cs.as_path(), Path::new(""));
        assert_eq!(cs.next(), None);
    }

    #[test]
    fn test_components_double_ended() {
        let path = Path::new("/home/user");
        let mut cs = path.components();
        assert_eq!(cs.as_path(), Path::new("/home/user"));
        assert_eq!(cs.next_back(), Some(Component::Normal(OsStr::new("user"))));
        assert_eq!(cs.as_path(), Path::new("/home"));
        assert_eq!(cs.next_back(), Some(Component::Normal(OsStr::new("home"))));
        assert_eq!(cs.as_path(), Path::new("/"));
        assert_eq!(cs.next_back(), Some(Component::RootDir));
        assert_eq!(cs.as_path(), Path::new(""));
        assert_eq!(cs.next_back(), None);
    }

    #[test]
    fn test_component_as_os_str() {
        let component = Component::Normal(OsStr::new("home"));
        assert_eq!(component.as_os_str(), OsStr::new("home"));

        let component = Component::RootDir;
        assert_eq!(component.as_os_str(), OsStr::new("/"));

        let component = Component::CurDir;
        assert_eq!(component.as_os_str(), OsStr::new("."));

        let component = Component::ParentDir;
        assert_eq!(component.as_os_str(), OsStr::new(".."));
    }

    #[track_caller]
    fn check_components<P>(path: P, expected: &[Component<'_>])
    where
        P: AsRef<Path>,
    {
        let path = path.as_ref();
        assert!(path.components().eq(expected.iter().copied()));
        assert!(path.components().rev().eq(expected.iter().copied().rev()));
    }

    #[test]
    fn test_components_empty_path() {
        check_components("", &[]);
    }

    #[test]
    fn test_components_root_path() {
        check_components("/", &[Component::RootDir]);
    }

    #[test]
    fn test_components_relative_path() {
        check_components(
            "home/user",
            &[
                Component::Normal(OsStr::new("home")),
                Component::Normal(OsStr::new("user")),
            ],
        );
    }

    #[test]
    fn test_components_with_dots() {
        check_components(
            "/home/./user/../docs",
            &[
                Component::RootDir,
                Component::Normal(OsStr::new("home")),
                Component::CurDir,
                Component::Normal(OsStr::new("user")),
                Component::ParentDir,
                Component::Normal(OsStr::new("docs")),
            ],
        );
    }

    #[test]
    fn test_components_with_trailing_slash() {
        let path = Path::new("/home/user/");
        let mut cs = path.components();
        assert_eq!(cs.next(), Some(Component::RootDir));
        assert_eq!(cs.next(), Some(Component::Normal(OsStr::new("home"))));
        assert_eq!(cs.next(), Some(Component::Normal(OsStr::new("user"))));
        assert_eq!(cs.next(), None);
    }

    #[test]
    fn test_components_with_multiple_slashes() {
        let path = Path::new("/home//user///docs");
        let mut cs = path.components();
        assert_eq!(cs.next(), Some(Component::RootDir));
        assert_eq!(cs.next(), Some(Component::Normal(OsStr::new("home"))));
        assert_eq!(cs.next(), Some(Component::Normal(OsStr::new("user"))));
        assert_eq!(cs.next(), Some(Component::Normal(OsStr::new("docs"))));
        assert_eq!(cs.next(), None);
    }

    #[test]
    fn test_components_with_only_dots() {
        let path = Path::new("././.");
        let mut cs = path.components();
        assert_eq!(cs.next(), Some(Component::CurDir));
        assert_eq!(cs.next(), Some(Component::CurDir));
        assert_eq!(cs.next(), Some(Component::CurDir));
        assert_eq!(cs.next(), None);
    }

    #[test]
    fn test_components_with_only_parent_dirs() {
        let path = Path::new("../..");
        let mut cs = path.components();
        assert_eq!(cs.next(), Some(Component::ParentDir));
        assert_eq!(cs.next(), Some(Component::ParentDir));
        assert_eq!(cs.next(), None);
    }

    #[test]
    fn test_components_with_mixed_dots_and_parent_dirs() {
        let path = Path::new("./.././../.");
        let mut cs = path.components();
        assert_eq!(cs.next(), Some(Component::CurDir));
        assert_eq!(cs.next(), Some(Component::ParentDir));
        assert_eq!(cs.next(), Some(Component::CurDir));
        assert_eq!(cs.next(), Some(Component::ParentDir));
        assert_eq!(cs.next(), Some(Component::CurDir));
        assert_eq!(cs.next(), None);
    }
}
