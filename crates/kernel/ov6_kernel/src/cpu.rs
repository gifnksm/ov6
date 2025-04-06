use core::{
    arch::asm,
    ptr::NonNull,
    sync::atomic::{AtomicBool, Ordering},
};

use ov6_types::process::ProcId;

use crate::{interrupt, param::NCPU, proc::Proc, sync::SpinLock};

static CPUS: [Cpu; NCPU] = [const { Cpu::new() }; NCPU];

/// Per-CPU state.
pub struct Cpu {
    /// The process running on this Cpu.
    proc: SpinLock<Option<(ProcId, NonNull<Proc>)>>,
    /// `true` if this CPU is idle.
    idle: AtomicBool,
}

unsafe impl Sync for Cpu {}

pub const INVALID_CPUID: usize = usize::MAX;

/// Returns current CPU's ID.
///
/// Must be called with interrupts disabled,
/// to prevent race with process being moved
/// to a different CPU.
#[track_caller]
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

pub fn is_idle(id: usize) -> bool {
    assert!(id < NCPU);
    CPUS[id].idle.load(Ordering::Relaxed)
}

impl Cpu {
    const fn new() -> Self {
        Self {
            proc: SpinLock::new(None),
            idle: AtomicBool::new(false),
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

    pub fn set_idle(&self, idle: bool) {
        self.idle.store(idle, Ordering::Relaxed);
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
