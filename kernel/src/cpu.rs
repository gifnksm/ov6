use core::{arch::asm, cell::UnsafeCell, ptr::NonNull};

use crate::{
    interrupt,
    param::NCPU,
    proc::{Context, Proc},
};

static CPUS: [Cpu; NCPU] = [const { Cpu::new() }; NCPU];

/// Per-CPU state.
pub struct Cpu {
    /// The process running on this Cpu, or null.
    pub proc: UnsafeCell<Option<NonNull<Proc>>>,
    /// switch() here to enter scheduler()
    pub context: UnsafeCell<Context>,
}

unsafe impl Sync for Cpu {}

/// Returns current CPU's ID.
///
/// Must be called with interrupts disabled,
/// to prevent race with process being moved
/// to a different CPU.
pub fn id() -> usize {
    assert!(!interrupt::is_enabled());

    let id: usize;
    unsafe { asm!("mv {}, tp", out(reg) id) };
    id
}

/// Stores current CPU's ID.
pub unsafe fn set_id(id: usize) {
    unsafe {
        asm!("mv tp, {}", in(reg) id);
    }
}

impl Cpu {
    const fn new() -> Self {
        Self {
            proc: UnsafeCell::new(None),
            context: UnsafeCell::new(Context::zeroed()),
        }
    }

    /// Returns this CPU's cpu struct.
    ///
    /// Interrupts must be disabled.
    pub fn current() -> &'static Self {
        assert!(!interrupt::is_enabled());

        let id = id();
        &CPUS[id]
    }
}
