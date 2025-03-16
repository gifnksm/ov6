//! A simple mutex API.
#![cfg_attr(any(not(feature = "std"), target_os = "none"), no_std)]

use core::ops::DerefMut;

/// A mutex.
pub trait Mutex {
    /// The type of the data that the mutex protects.
    type Data;

    /// The type of the guard that the `lock` method returns.
    type Guard<'a>: DerefMut<Target = Self::Data>
    where
        Self: 'a;

    /// Creates a new mutex.
    fn new(data: Self::Data) -> Self;

    /// Locks the mutex.
    fn lock(&self) -> Self::Guard<'_>;
}

#[cfg(all(feature = "std", not(target_os = "none")))]
impl<T> Mutex for std::sync::Mutex<T> {
    type Data = T;
    type Guard<'a>
        = std::sync::MutexGuard<'a, T>
    where
        T: 'a;

    fn new(data: Self::Data) -> Self {
        Self::new(data)
    }

    fn lock(&self) -> Self::Guard<'_> {
        self.lock().unwrap()
    }
}
