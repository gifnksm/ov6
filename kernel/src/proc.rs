use core::ffi::c_int;

mod ffi {

    use super::*;

    unsafe extern "C" {
        pub type Proc;
        pub fn mycpu() -> *mut Cpu;
    }
}

/// Saved registers for kernel context switches.
#[repr(C)]
struct Context {
    ra: u64,
    sp: u64,

    // callee-saved
    s0: u64,
    s1: u64,
    s2: u64,
    s3: u64,
    s4: u64,
    s5: u64,
    s6: u64,
    s7: u64,
    s8: u64,
    s9: u64,
    s10: u64,
    s11: u64,
}

/// Per-CPU state.
#[repr(C)]
pub struct Cpu {
    /// The process running on thie Cpu, or null.
    proc: *mut ffi::Proc,
    /// swtch() here to enter scheduler()
    context: Context,
    /// Depth of `push_off()` nesting.
    pub noff: c_int,
    /// Were interrupts enabled before `push_off()`?
    pub intena: c_int,
}

impl Cpu {
    #[inline]
    pub fn mycpu() -> *mut Self {
        unsafe { ffi::mycpu() }
    }
}
