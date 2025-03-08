use core::{arch::asm, ptr::NonNull};

use ov6_types::process::ProcId;

use crate::{interrupt, param::NCPU, proc::Proc, sync::SpinLock};

static CPUS: [Cpu; NCPU] = [const { Cpu::new() }; NCPU];

/// Per-CPU state.
pub struct Cpu {
    /// The process running on this Cpu.
    proc: SpinLock<Option<(ProcId, NonNull<Proc>)>>,
}

unsafe impl Sync for Cpu {}

pub const INVALID_CPUID: usize = usize::MAX;

/// Returns current CPU's ID.
///
/// Must be called with interrupts disabled,
/// to prevent race with process being moved
/// to a different CPU.
pub fn id() -> usize {
    assert!(!interrupt::is_enabled());

    let id: usize;
    unsafe {
        asm!("mv {}, tp", out(reg) id);
    }
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
            proc: SpinLock::new(None),
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

    pub fn set_proc(&self, p: Option<(ProcId, &Proc)>) {
        assert!(!interrupt::is_enabled());

        *self.proc.try_lock().unwrap() = p.map(|(pid, p)| (pid, NonNull::from(p)));
    }

    pub fn pid(&self) -> Option<ProcId> {
        assert!(!interrupt::is_enabled());

        self.proc.try_lock().unwrap().map(|p| p.0)
    }

    pub fn proc(&self) -> Option<NonNull<Proc>> {
        assert!(!interrupt::is_enabled());

        self.proc.try_lock().unwrap().map(|p| p.1)
    }
}
