//! A simple mutex API.
#![no_std]

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
