use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    sync::atomic::{AtomicBool, Ordering},
};

use dataview::Pod;

/// A synchronization primitive which can be written to only once.
pub struct Once<T> {
    initialized: AtomicBool,
    value: UnsafeCell<MaybeUninit<T>>,
}

unsafe impl<T> Sync for Once<T> where T: Send {}

impl<T> Once<T> {
    /// Creates a new empty cell.
    pub const fn new() -> Self {
        Self {
            initialized: AtomicBool::new(false),
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    /// Initializes the cell.
    ///
    /// `value` will be dropped when this cell will be dropped.
    ///
    /// # Panics
    ///
    /// This function will panic if the cell already initialized.
    pub fn init(&self, value: T) {
        self.initialized
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .expect("Once::init should be called at most once");

        unsafe {
            (*self.value.get()).write(value);
        }
    }

    /// Initializes the cell by reference.
    ///
    /// This function is useful when the value is large and we want to avoid copying it.
    ///
    /// # Panics
    ///
    /// This function will panic if the cell already initialized.
    pub fn init_by_ref(&self, value: &T)
    where
        T: Pod,
    {
        self.initialized
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .expect("Once::init should be called at most once");

        unsafe {
            (*self.value.get()).as_mut_ptr().copy_from(value, 1);
        }
    }

    /// Gets the reference of the contents of the cell.
    ///
    /// # Panics
    ///
    /// This function will panic if the cell is empty.
    pub fn get(&self) -> &T {
        if !self.initialized.load(Ordering::Acquire) {
            panic!("Once is not initialized");
        }

        unsafe { (*self.value.get()).assume_init_ref() }
    }
}

impl<T> Drop for Once<T> {
    fn drop(&mut self) {
        // Drops `value` only if the cell is initialized.
        if self.initialized.load(Ordering::Acquire) {
            unsafe {
                (*self.value.get()).assume_init_drop();
            }
        }
    }
}
