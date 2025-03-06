//! A synchronization primitive which can be written to only once.

#![cfg_attr(not(test), no_std)]

use core::{
    cell::UnsafeCell,
    error::Error,
    fmt,
    mem::MaybeUninit,
    sync::atomic::{AtomicBool, Ordering},
};

use dataview::Pod;

/// A synchronization primitive which can be written to only once.
pub struct OnceInit<T> {
    initializing: AtomicBool,
    initialized: AtomicBool,
    value: UnsafeCell<MaybeUninit<T>>,
}

unsafe impl<T> Sync for OnceInit<T> where T: Send {}

impl<T> Default for OnceInit<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> fmt::Debug for OnceInit<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut f = f.debug_tuple("OnceInit");
        if let Ok(value) = self.try_get() {
            f.field(&value);
        } else {
            f.field(&format_args!("<uninit>"));
        }
        f.finish()
    }
}

impl<T> OnceInit<T> {
    /// Creates a new uninitialized cell.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            initializing: AtomicBool::new(false),
            initialized: AtomicBool::new(false),
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    pub fn try_init_with<F>(&self, f: F) -> Result<(), InitError>
    where
        F: FnOnce() -> T,
    {
        if self
            .initializing
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return Err(InitError::AlreadyInitialized);
        }

        unsafe {
            (*self.value.get()).write(f());
        }

        self.initialized.store(true, Ordering::Release);

        Ok(())
    }

    /// Initializes the cell.
    ///
    /// `value` will be dropped when this cell will be dropped.
    /// Returns `Err()` if the cell already initialized.
    pub fn try_init(&self, value: T) -> Result<(), T> {
        if self
            .initializing
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return Err(value);
        }

        unsafe {
            (*self.value.get()).write(value);
        }

        self.initialized.store(true, Ordering::Release);

        Ok(())
    }

    /// Initializes the cell by reference.
    ///
    /// This function is useful when the value is large and we want to avoid copying it.
    /// Returns `Err()` if the cell already initialized.
    pub fn try_init_by_ref(&self, value: &T) -> Result<(), InitError>
    where
        T: Pod,
    {
        if self
            .initializing
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return Err(InitError::AlreadyInitialized);
        }

        unsafe {
            (*self.value.get()).as_mut_ptr().copy_from(value, 1);
        }

        self.initialized.store(true, Ordering::Release);

        Ok(())
    }

    /// Initializes the cell.
    ///
    /// `value` will be dropped when this cell will be dropped.
    ///
    /// # Panics
    ///
    /// Panics if the cell already initialized.
    #[track_caller]
    pub fn init(&self, value: T) {
        // `Result::expect` requires `T: Debug`, so we can't use it here
        assert!(
            self.try_init(value).is_ok(),
            "OnceInit should be initialized at most once"
        );
    }

    /// Initializes the cell by reference.
    ///
    /// This function is useful when the value is large and we want to avoid copying it.
    ///
    /// # Panics
    ///
    /// Panics if the cell already initialized.
    #[track_caller]
    pub fn init_by_ref(&self, value: &T)
    where
        T: Pod,
    {
        assert!(
            self.try_init_by_ref(value).is_ok(),
            "OnceInit should be initialized at most once"
        );
    }

    /// Gets the reference of the contents of the cell.
    ///
    /// # Panics
    ///
    /// This function will panic if the cell is not initialized.
    #[track_caller]
    pub fn get(&self) -> &T {
        self.try_get()
            .expect("Once should be initialized before get")
    }

    /// Gets the reference of the contents of the cell.
    ///
    /// Returns `Err(())` if the cell is not initialized.
    #[track_caller]
    pub fn try_get(&self) -> Result<&T, GetError> {
        if !self.initialized.load(Ordering::Acquire) {
            return Err(GetError::NotInitialized);
        }

        Ok(unsafe { (*self.value.get()).assume_init_ref() })
    }
}

impl<T> Drop for OnceInit<T> {
    fn drop(&mut self) {
        // Drops `value` only if the cell is initialized.
        if self.initialized.load(Ordering::Acquire) {
            unsafe {
                (*self.value.get()).assume_init_drop();
            }
        }
    }
}

/// An error returns from [`OnceInit`] initialize functions.
#[derive(Debug)]
pub enum InitError {
    /// [`OnceInit`] is already initialized.
    AlreadyInitialized,
}

impl fmt::Display for InitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyInitialized => fmt::Display::fmt("already initialized", f),
        }
    }
}

impl Error for InitError {}

/// An error returns from [`OnceInit`] get functions.
#[derive(Debug)]
pub enum GetError {
    /// [`OnceInit`] is already initialized.
    NotInitialized,
}

impl fmt::Display for GetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInitialized => fmt::Display::fmt("not initialized", f),
        }
    }
}

impl Error for GetError {}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Barrier},
        thread,
    };

    use super::*;

    #[test]
    fn second_init_should_fail() {
        let once = OnceInit::new();

        once.init(123);
        assert!(once.try_init(455).is_err());

        assert_eq!(once.get(), &123);
    }

    #[test]
    fn debug_print() {
        let once = OnceInit::new();
        assert_eq!(format!("{once:?}"), "OnceInit(<uninit>)");
        once.init(123);
        assert_eq!(format!("{once:?}"), "OnceInit(123)");
    }

    #[test]
    fn concurrent_initialization_should_return_first_success() {
        let once = Arc::new(OnceInit::new());
        let barrier = Arc::new(Barrier::new(10));

        let mut threads = vec![];
        for i in 0..10 {
            let once = Arc::clone(&once);
            let barrier = Arc::clone(&barrier);
            let handle = thread::spawn(move || {
                barrier.wait();
                once.try_init(i).ok().map(|()| i)
            });
            threads.push(handle);
        }

        let mut result = None;
        for handle in threads {
            if let Some(res) = handle.join().unwrap() {
                assert!(result.is_none());
                result = Some(res);
            }
        }
        assert_eq!(*once.get(), result.unwrap());
    }

    #[test]
    fn init_by_ref() {
        let once = OnceInit::new();
        let value = 123;
        once.init_by_ref(&value);
        assert_eq!(once.get(), &123);
    }

    #[test]
    fn try_init_by_ref_fails_if_already_initialized() {
        let once = OnceInit::new();
        let value1 = 123;
        let value2 = 456;

        once.init_by_ref(&value1);
        once.try_init_by_ref(&value2).unwrap_err();
        assert_eq!(once.get(), &123);
    }

    #[test]
    fn get_fails_if_not_initialized() {
        let once = OnceInit::<i32>::new();
        once.try_get().unwrap_err();
    }
}
