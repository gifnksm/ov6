use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};

use riscv::register::sstatus;

use crate::proc::Cpu;

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

    /// Acquires the lock.
    ///
    /// Loops (spins) until the lock is acquired.
    pub fn acquire(&self) {
        push_off(); // disable interrupts to avoid deadlock.

        assert!(!self.holding());

        // `Ordering::Acquire` tells the compiler and the processor to not move loads or stores
        // past this point, to ensure that the critical section's memory
        // references happen strictly after the lock is acquired.
        // On RISC-V, this emits a fence instruction.
        while self.locked.swap(true, Ordering::Acquire) {}

        // Record info about lock acquisition for holding() and debugging.
        unsafe {
            *self.cpu.get() = Some(Cpu::mycpu());
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

        pop_off();
    }

    /// Checks whether this cpu is holding the lock.
    ///
    /// Interrupts must be off.
    pub fn holding(&self) -> bool {
        assert!(!sstatus::read().sie());
        self.locked.load(Ordering::Relaxed)
            && unsafe { *self.cpu.get() }
                .map(|c| ptr::eq(c, Cpu::mycpu()))
                .unwrap_or(false)
    }
}

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

    /// Acquires the lock.
    ///
    /// Loops (spins) until the lock is acquired.
    pub fn lock(&self) -> SpinLockGuard<T> {
        self.lock.acquire();
        SpinLockGuard { lock: self }
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

// push_off/pop_off are like clear_sie()/set_sie() except that they are matched:
// it takes two pop_off()s to undo two push_off()s.  Also, if interrupts
// are initially off, then push_off, pop_off leaves them off.

pub fn push_off() {
    let old = sstatus::read().sie();

    unsafe {
        sstatus::clear_sie();
        if *Cpu::mycpu().noff.get() == 0 {
            *Cpu::mycpu().intena.get() = old.into();
        }
        *Cpu::mycpu().noff.get() += 1;
    }
}

pub fn pop_off() {
    unsafe {
        let c = Cpu::mycpu();
        assert!(!sstatus::read().sie());
        assert!(*c.noff.get() > 0);
        *c.noff.get() -= 1;
        if *c.noff.get() == 0 && *c.intena.get() != 0 {
            sstatus::set_sie();
        }
    }
}
