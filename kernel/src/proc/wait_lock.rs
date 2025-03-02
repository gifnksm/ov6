use core::{cell::UnsafeCell, ptr::NonNull};

use crate::sync::{SpinLock, SpinLockGuard};

use super::Proc;

pub(super) struct WaitLock {}

/// Helps ensure that wakeups of wait()ing
/// parents are not lost.
///
/// Helps obey the memory model when using `Proc::parent`.
/// Must be acquired before any `Proc::lock`.
static WAIT_LOCK: SpinLock<WaitLock> = SpinLock::new(WaitLock {});

pub(super) fn lock() -> SpinLockGuard<'static, WaitLock> {
    WAIT_LOCK.lock()
}

pub(super) struct Parent {
    parent: UnsafeCell<Option<NonNull<Proc>>>,
}

impl Parent {
    pub(super) const fn new() -> Self {
        Self {
            parent: UnsafeCell::new(None),
        }
    }

    pub(super) fn get<'a>(
        &self,
        _wait_lock: &mut SpinLockGuard<'a, WaitLock>,
    ) -> &'a Option<NonNull<Proc>> {
        unsafe { &*self.parent.get() }
    }

    pub(super) fn set(
        &self,
        parent: Option<NonNull<Proc>>,
        _wait_lock: &mut SpinLockGuard<WaitLock>,
    ) {
        unsafe {
            *self.parent.get() = parent;
        }
    }

    pub(super) unsafe fn reset(&self) {
        unsafe {
            *self.parent.get() = None;
        }
    }
}
