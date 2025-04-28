use core::{arch::naked_asm, mem::offset_of};

use ov6_kernel_params::NCPU;
use riscv::asm;

use super::{PROC, Proc, ProcSharedData, ProcState};
use crate::{
    cpu::{self, Cpu},
    interrupt,
    sync::SpinLockGuard,
};

/// Scheduler context.
///
/// Call `switch()` here to enter scheduler.
static mut SCHED_CONTEXT: [Context; NCPU] = [const { Context::zeroed() }; NCPU];

/// Saved registers for kernel context switches.
pub struct Context {
    pub(super) ra: usize,
    pub(super) sp: usize,

    // callee-saved
    s0: usize,
    s1: usize,
    s2: usize,
    s3: usize,
    s4: usize,
    s5: usize,
    s6: usize,
    s7: usize,
    s8: usize,
    s9: usize,
    s10: usize,
    s11: usize,
}

impl Context {
    pub(super) const fn zeroed() -> Self {
        Self {
            ra: 0,
            sp: 0,
            s0: 0,
            s1: 0,
            s2: 0,
            s3: 0,
            s4: 0,
            s5: 0,
            s6: 0,
            s7: 0,
            s8: 0,
            s9: 0,
            s10: 0,
            s11: 0,
        }
    }

    pub(super) const fn clear(&mut self) {
        *self = Self::zeroed();
    }
}

/// Per-CPU process scheduler.
///
/// Each CPU calls `schedule()` after setting itself up.
/// Scheduler never returns.
///
/// It loops doing:
///
/// - choose a process to run.
/// - switch to start running that process.
/// - eventually that process transfers control via switch back to the
///   scheduler.
pub fn schedule() -> ! {
    let cpu = Cpu::current();
    cpu.set_proc(None);
    let cpuid = cpu::id();

    loop {
        // The most recent process to run may have had interrupts
        // turned off; enable them to avoid a deadlock if all
        // processes are waiting.
        interrupt::enable();

        let mut found = false;
        for p in &PROC {
            let Ok(mut shared) = p.shared.try_lock() else {
                // The process is running on another CPU.
                continue;
            };
            if shared.state != ProcState::Runnable {
                drop(shared);
                continue;
            }

            // Switch to chosen process. It is the process's job
            // to release its lock and then reacquire it
            // before jumping back to us.
            shared.state = ProcState::Running;
            cpu.set_proc(Some((shared.pid.unwrap(), p)));
            unsafe {
                switch(&raw mut SCHED_CONTEXT[cpuid], &raw const shared.context);
            }

            // Process is done running for now.
            // It should have changed its p->state before coming back.
            cpu.set_proc(None);
            found = true;
            drop(shared);
        }

        if !found {
            // nothing to run, stop running on this core until an interrupt.
            interrupt::enable();
            cpu.set_idle(true);
            asm::wfi();
            cpu.set_idle(false);
        }
    }
}

/// Gives up the CPU for one shceduling round.
pub fn yield_(p: &Proc) {
    let mut shared = p.shared.lock();
    assert!(matches!(shared.state, ProcState::Running));
    shared.state = ProcState::Runnable;
    sched(&mut shared);
    drop(shared);
}

/// Switch to shcduler.
///
/// Must hold only `Proc::lock` and  have changed `proc->state`.
///
/// Saves and restores `Cpu:intena` because `inteta` is a property of this
/// kernel thread, not this CPU. It should be `Proc::intena` and `Proc::noff`,
/// but that would break in the few places where a lock is held but there's no
/// process.
pub(super) fn sched(shared: &mut SpinLockGuard<ProcSharedData>) {
    assert_eq!(interrupt::disabled_depth(), 1);
    assert_ne!(shared.state, ProcState::Running);
    assert!(!interrupt::is_enabled());
    let cpuid = cpu::id();

    let int_enabled = interrupt::is_enabled_before_push();
    unsafe {
        switch(&raw mut shared.context, &raw const SCHED_CONTEXT[cpuid]);
    }
    unsafe {
        interrupt::force_set_before_push(int_enabled);
    }
}

/// Saves current registers in `old`, loads from `new`.
#[unsafe(naked)]
unsafe extern "C" fn switch(old: *mut Context, new: *const Context) {
    naked_asm!(
        "sd ra, {c_ra}(a0)",
        "sd sp, {c_sp}(a0)",
        "sd s0, {c_s0}(a0)",
        "sd s1, {c_s1}(a0)",
        "sd s2, {c_s2}(a0)",
        "sd s3, {c_s3}(a0)",
        "sd s4, {c_s4}(a0)",
        "sd s5, {c_s5}(a0)",
        "sd s6, {c_s6}(a0)",
        "sd s7, {c_s7}(a0)",
        "sd s8, {c_s8}(a0)",
        "sd s9, {c_s9}(a0)",
        "sd s10, {c_s10}(a0)",
        "sd s11, {c_s11}(a0)",
        "ld ra, {c_ra}(a1)",
        "ld sp, {c_sp}(a1)",
        "ld s0, {c_s0}(a1)",
        "ld s1, {c_s1}(a1)",
        "ld s2, {c_s2}(a1)",
        "ld s3, {c_s3}(a1)",
        "ld s4, {c_s4}(a1)",
        "ld s5, {c_s5}(a1)",
        "ld s6, {c_s6}(a1)",
        "ld s7, {c_s7}(a1)",
        "ld s8, {c_s8}(a1)",
        "ld s9, {c_s9}(a1)",
        "ld s10, {c_s10}(a1)",
        "ld s11, {c_s11}(a1)",
        "ret",
        c_ra = const offset_of!(Context, ra),
        c_sp = const offset_of!(Context, sp),
        c_s0 = const offset_of!(Context, s0),
        c_s1 = const offset_of!(Context, s1),
        c_s2 = const offset_of!(Context, s2),
        c_s3 = const offset_of!(Context, s3),
        c_s4 = const offset_of!(Context, s4),
        c_s5 = const offset_of!(Context, s5),
        c_s6 = const offset_of!(Context, s6),
        c_s7 = const offset_of!(Context, s7),
        c_s8 = const offset_of!(Context, s8),
        c_s9 = const offset_of!(Context, s9),
        c_s10 = const offset_of!(Context, s10),
        c_s11 = const offset_of!(Context, s11),
    )
}
