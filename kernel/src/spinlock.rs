use core::{
    cell::UnsafeCell,
    ffi::{CStr, c_char, c_uint},
    ops::{Deref, DerefMut},
    ptr,
    sync::atomic::{AtomicU32, Ordering},
};

use riscv::register::sstatus;

use crate::proc::Cpu;

#[repr(C)]
pub struct SpinLock {
    locked: AtomicU32,
    name: UnsafeCell<*const c_char>,
    cpu: UnsafeCell<*mut Cpu>,
}

const _: () = assert!(size_of::<AtomicU32>() == size_of::<c_uint>());

mod ffi {
    use super::*;

    #[unsafe(no_mangle)]
    extern "C" fn initlock(lock: *mut SpinLock, name: *const c_char) {
        unsafe { (*lock).init(name) }
    }

    #[unsafe(no_mangle)]
    extern "C" fn acquire(lock: *const SpinLock) {
        unsafe { (*lock).acquire() }
    }

    #[unsafe(no_mangle)]
    extern "C" fn release(lock: *const SpinLock) {
        unsafe { (*lock).release() }
    }
}

unsafe impl Sync for SpinLock {}

impl SpinLock {
    pub const fn new(name: &'static CStr) -> Self {
        Self {
            locked: AtomicU32::new(0),
            name: UnsafeCell::new(name.as_ptr()),
            cpu: UnsafeCell::new(ptr::null_mut()),
        }
    }

    pub unsafe fn init(&mut self, name: *const c_char) {
        *self.name.get_mut() = name;
        self.locked.store(0, Ordering::Relaxed);
        *self.cpu.get_mut() = ptr::null_mut();
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
        while self.locked.swap(1, Ordering::Acquire) != 0 {}

        // Record info about lock acquisition for holding() and debugging.
        unsafe {
            *self.cpu.get() = Cpu::mycpu();
        }
    }

    /// Releases the lock.
    pub fn release(&self) {
        assert!(self.holding());

        unsafe {
            *self.cpu.get() = ptr::null_mut();
        }

        // `Ordering::Release` tells the compiler and the CPU to not move loads or stores
        // past this point, to ensure that all the stores in the critical
        // section are visible to other CPUs before the lock is released,
        // and that loads in the critical section occur strictly before
        // the locks is released.
        // On RISC-V, this emits a fence instruction.
        self.locked.store(0, Ordering::Release);

        pop_off();
    }

    /// Checks whether this cpu is holding the lock.
    ///
    /// Interrupts must be off.
    pub fn holding(&self) -> bool {
        assert!(!sstatus::read().sie());
        self.locked.load(Ordering::Relaxed) != 0 && unsafe { *self.cpu.get() } == Cpu::mycpu()
    }
}

pub struct Mutex<T> {
    lock: SpinLock,
    value: UnsafeCell<T>,
}

unsafe impl<T> Sync for Mutex<T> where T: Send {}

impl<T> Mutex<T> {
    pub const fn new(value: T) -> Self {
        Self {
            lock: SpinLock {
                locked: AtomicU32::new(0),
                name: UnsafeCell::new(ptr::null()),
                cpu: UnsafeCell::new(ptr::null_mut()),
            },
            value: UnsafeCell::new(value),
        }
    }

    /// Acquires the lock.
    ///
    /// Loops (spins) until the lock is acquired.
    pub fn lock(&self) -> MutexGuard<T> {
        self.lock.acquire();
        MutexGuard { lock: self }
    }
}

pub struct MutexGuard<'a, T> {
    lock: &'a Mutex<T>,
}

unsafe impl<T> Send for MutexGuard<'_, T> where T: Send {}
unsafe impl<T> Sync for MutexGuard<'_, T> where T: Sync {}

impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.lock.release();
    }
}

impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T> MutexGuard<'_, T> {
    pub unsafe fn spinlock(&self) -> &SpinLock {
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
        if (*Cpu::mycpu()).noff == 0 {
            (*Cpu::mycpu()).intena = old.into();
        }
        (*Cpu::mycpu()).noff += 1;
    }
}

pub fn pop_off() {
    unsafe {
        let c = Cpu::mycpu();
        assert!(!sstatus::read().sie());
        assert!((*c).noff > 0);
        (*c).noff -= 1;
        if (*c).noff == 0 && (*c).intena != 0 {
            sstatus::set_sie();
        }
    }
}
