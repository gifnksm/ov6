use core::{
    ffi::CStr,
    slice,
    sync::atomic::{AtomicPtr, AtomicUsize, Ordering},
};

use crate::{error::Error, os::xv6::syscall};

pub(crate) static ARGC: AtomicUsize = AtomicUsize::new(0);
pub(crate) static ARGV: AtomicPtr<*const u8> = AtomicPtr::new(core::ptr::null_mut());

pub(crate) fn set_args(argc: usize, argv: *const *const u8) {
    ARGC.store(argc, Ordering::Relaxed);
    ARGV.store(argv.cast_mut(), Ordering::Relaxed);
}

fn argv() -> &'static [*const u8] {
    let argc = ARGC.load(Ordering::Relaxed);
    let argv = ARGV.load(Ordering::Relaxed);
    if argv.is_null() {
        return &[];
    }

    unsafe { slice::from_raw_parts(argv, argc) }
}

pub fn arg0() -> &'static str {
    let arg0 = argv().first().expect("argc should be greater than 1");
    unsafe { CStr::from_ptr(*arg0).to_str().unwrap() }
}

pub fn args() -> Args {
    let args = argv();
    let mut iter = args.iter();
    iter.next(); // Skip the program name
    Args { iter }
}

pub fn args_cstr() -> ArgsCStr {
    let args = argv();
    let mut iter = args.iter();
    iter.next(); // Skip the program name
    ArgsCStr { iter }
}

pub struct Args {
    iter: slice::Iter<'static, *const u8>,
}

impl Iterator for Args {
    type Item = &'static str;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter
            .next()
            .map(|&arg| unsafe { CStr::from_ptr(arg).to_str().unwrap() })
    }
}

impl ExactSizeIterator for Args {
    fn len(&self) -> usize {
        self.iter.len()
    }
}

pub struct ArgsCStr {
    iter: slice::Iter<'static, *const u8>,
}

impl Iterator for ArgsCStr {
    type Item = &'static CStr;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|&arg| unsafe { CStr::from_ptr(arg) })
    }
}

impl ExactSizeIterator for ArgsCStr {
    fn len(&self) -> usize {
        self.iter.len()
    }
}

pub fn set_current_directory(path: &CStr) -> Result<(), Error> {
    syscall::chdir(path)
}
