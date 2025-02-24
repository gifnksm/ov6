use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    ptr,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

use mutex_api::Mutex;

use crate::{cpu::Cpu, error::Error, interrupt, proc};

#[derive(Default)]
pub struct RawSpinLock {
    locked: AtomicBool,
    cpu: UnsafeCell<Option<&'static Cpu>>,
}

unsafe impl Sync for RawSpinLock {}

impl RawSpinLock {
    pub const fn new() -> Self {
        Self {
            locked: AtomicBool::new(false),
            cpu: UnsafeCell::new(None),
        }
    }

    pub fn try_acquire(&self) -> Result<(), Error> {
        // disable interrupts to avoid deadlock.
        interrupt::push_disabled().forget(); // drop re-enables interrupts, so we must forget it here.

        assert!(!self.holding());

        // `Ordering::Acquire` tells the compiler and the processor to not move loads or stores
        // past this point, to ensure that the critical section's memory
        // references happen strictly after the lock is acquired.
        // On RISC-V, this emits a fence instruction.
        if self.locked.swap(true, Ordering::Acquire) {
            return Err(Error::Unknown);
        }

        // Record info about lock acquisition for holding() and debugging.
        unsafe {
            *self.cpu.get() = Some(Cpu::current());
        }

        Ok(())
    }

    /// Acquires the lock.
    ///
    /// Loops (spins) until the lock is acquired.
    pub fn acquire(&self) {
        // disable interrupts to avoid deadlock.
        interrupt::push_disabled().forget(); // drop re-enables interrupts, so we must forget it here.

        assert!(!self.holding());

        // `Ordering::Acquire` tells the compiler and the processor to not move loads or stores
        // past this point, to ensure that the critical section's memory
        // references happen strictly after the lock is acquired.
        // On RISC-V, this emits a fence instruction.
        while self.locked.swap(true, Ordering::Acquire) {}

        // Record info about lock acquisition for holding() and debugging.
        unsafe {
            *self.cpu.get() = Some(Cpu::current());
        }
    }

    /// Releases the lock.
    pub fn release(&self) {
        assert!(self.holding());

        unsafe {
            *self.cpu.get() = None;
        }

        // `Ordering::Release` tells the compiler and the CPU to not move loads or stores
        // past this point, to ensure that all the stores in the critical
        // section are visible to other CPUs before the lock is released,
        // and that loads in the critical section occur strictly before
        // the locks is released.
        // On RISC-V, this emits a fence instruction.
        self.locked.store(false, Ordering::Release);

        unsafe {
            interrupt::pop_disabled();
        }
    }

    /// Checks whether this cpu is holding the lock.
    ///
    /// Interrupts must be off.
    pub fn holding(&self) -> bool {
        assert!(!interrupt::is_enabled());
        self.locked.load(Ordering::Relaxed)
            && unsafe { *self.cpu.get() }
                .map(|c| ptr::eq(c, Cpu::current()))
                .unwrap_or(false)
    }
}

#[derive(Default)]
pub struct SpinLock<T> {
    lock: RawSpinLock,
    value: UnsafeCell<T>,
}

unsafe impl<T> Sync for SpinLock<T> where T: Send {}

impl<T> SpinLock<T> {
    pub const fn new(value: T) -> Self {
        Self {
            lock: RawSpinLock::new(),
            value: UnsafeCell::new(value),
        }
    }

    // /// Acquires the lock.
    // ///
    // /// Loops (spins) until the lock is acquired.
    // pub fn try_lock(&self) -> Result<SpinLockGuard<T>, Error> {
    //     self.lock.try_acquire()?;
    //     Ok(SpinLockGuard { lock: self })
    // }

    /// Acquires the lock.
    ///
    /// Loops (spins) until the lock is acquired.
    pub fn lock(&self) -> SpinLockGuard<T> {
        self.lock.acquire();
        SpinLockGuard { lock: self }
    }
}

impl<T> Mutex for SpinLock<T> {
    type Data = T;
    type Guard<'a>
        = SpinLockGuard<'a, T>
    where
        T: 'a;

    fn new(data: Self::Data) -> Self {
        Self::new(data)
    }

    fn lock(&self) -> Self::Guard<'_> {
        self.lock()
    }
}

pub struct SpinLockGuard<'a, T> {
    lock: &'a SpinLock<T>,
}

unsafe impl<T> Send for SpinLockGuard<'_, T> where T: Send {}
unsafe impl<T> Sync for SpinLockGuard<'_, T> where T: Sync {}

impl<T> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.lock.release();
    }
}

impl<T> Deref for SpinLockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T> SpinLockGuard<'_, T> {
    pub unsafe fn spinlock(&self) -> &RawSpinLock {
        &self.lock.lock
    }
}

pub struct SpinLockCondVar {
    counter: AtomicU64,
}

impl SpinLockCondVar {
    pub const fn new() -> Self {
        Self {
            counter: AtomicU64::new(0),
        }
    }

    pub fn wait<'a, T>(&self, mut guard: SpinLockGuard<'a, T>) -> SpinLockGuard<'a, T> {
        let counter = self.counter.load(Ordering::Relaxed);
        loop {
            proc::sleep(ptr::from_ref(&self.counter).cast(), &mut guard);
            if counter != self.counter.load(Ordering::Relaxed) {
                break;
            }
        }
        guard
    }

    pub fn notify(&self) {
        self.counter.fetch_add(1, Ordering::Relaxed);
        proc::wakeup(ptr::from_ref(&self.counter).cast());
    }
}
