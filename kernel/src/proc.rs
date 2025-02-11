use core::{
    arch::asm,
    cmp,
    ffi::{CStr, c_char, c_int, c_void},
    fmt,
    ops::Range,
    ptr::{self, NonNull},
    slice,
    sync::atomic::{AtomicBool, AtomicI32, Ordering},
};

use riscv::register::sstatus;

use crate::{
    file::{self, File, Inode},
    fs, kalloc, log,
    memlayout::{TRAMPOLINE, TRAPFRAME, kstack},
    param::{NCPU, NOFILE, NPROC, ROOTDEV},
    println,
    spinlock::{self, MutexGuard, SpinLock},
    switch, trampoline, trap,
    vm::{self, PAGE_SIZE, PageTable, PhysAddr, PtEntryFlags, VirtAddr},
};

static mut CPUS: [Cpu; NCPU] = [const { Cpu::zero() }; NCPU];
pub static mut PROC: [Proc; NPROC] = [const { Proc::new() }; NPROC];
pub static mut INITPROC: Option<NonNull<Proc>> = None;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct ProcId(i32);

impl fmt::Display for ProcId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// Helps ensure that wakeups of wait()ing
/// parents are not lost.
///
/// Helps obey the memory model when using `Proc::parent`.
/// Must be acquired before any `Proc::lock`.
static WAIT_LOCK: SpinLock = SpinLock::new(c"wait_lock");

mod ffi {
    use crate::spinlock::SpinLock;

    use super::*;

    #[unsafe(no_mangle)]
    extern "C" fn cpuid() -> c_int {
        super::cpuid()
    }

    #[unsafe(no_mangle)]
    extern "C" fn myproc() -> *mut Proc {
        super::Proc::myproc().map_or(ptr::null_mut(), ptr::from_mut)
    }

    #[unsafe(no_mangle)]
    extern "C" fn proc_pagetable(p: *mut Proc) -> *mut PageTable {
        super::create_pagetable(unsafe { p.as_mut().unwrap() })
            .map(|p| p.as_ptr())
            .unwrap_or_else(ptr::null_mut)
    }

    #[unsafe(no_mangle)]
    extern "C" fn proc_freepagetable(p: *mut PageTable, sz: u64) {
        super::free_pagetable(NonNull::new(p).unwrap(), sz as usize);
    }

    #[unsafe(no_mangle)]
    extern "C" fn growproc(n: c_int) -> c_int {
        match super::grow_proc(n as isize) {
            Ok(()) => 0,
            Err(()) => -1,
        }
    }

    #[unsafe(no_mangle)]
    extern "C" fn fork() -> c_int {
        match super::fork() {
            Some(pid) => pid.0,
            None => -1,
        }
    }

    #[unsafe(no_mangle)]
    extern "C" fn exit(status: c_int) -> ! {
        super::exit(status)
    }

    #[unsafe(no_mangle)]
    extern "C" fn wait(addr: u64) -> c_int {
        let addr = VirtAddr::new(addr as usize);
        match super::wait(addr) {
            Ok(pid) => pid.0,
            Err(()) => -1,
        }
    }

    #[unsafe(no_mangle)]
    extern "C" fn sleep(chan: *const c_void, lk: *mut SpinLock) {
        unsafe { super::sleep_raw(chan, lk.as_ref().unwrap()) }
    }

    #[unsafe(no_mangle)]
    extern "C" fn wakeup(chan: *const c_void) {
        super::wakeup(chan)
    }

    #[unsafe(no_mangle)]
    extern "C" fn kill(pid: c_int) -> c_int {
        let pid = ProcId(pid);
        match super::kill(pid) {
            Ok(()) => 0,
            Err(()) => -1,
        }
    }

    #[unsafe(no_mangle)]
    extern "C" fn killed(p: *mut Proc) -> c_int {
        let p = unsafe { p.as_mut().unwrap() };
        p.killed() as c_int
    }

    #[unsafe(no_mangle)]
    extern "C" fn either_copyout(user_dst: c_int, dst: u64, src: *const c_void, len: u64) -> c_int {
        let user_dst = user_dst != 0;
        let src = unsafe { slice::from_raw_parts(src.cast(), len as usize) };
        match super::either_copy_out(user_dst, dst as usize, src) {
            Ok(()) => 0,
            Err(()) => -1,
        }
    }

    #[unsafe(no_mangle)]
    extern "C" fn either_copyin(dst: *mut c_void, user_src: c_int, src: u64, len: u64) -> c_int {
        let user_src = user_src != 0;
        let dst = unsafe { slice::from_raw_parts_mut(dst.cast(), len as usize) };
        match super::either_copy_in(dst, user_src, src as usize) {
            Ok(()) => 0,
            Err(()) => -1,
        }
    }
}

/// Saved registers for kernel context switches.
#[repr(C)]
pub struct Context {
    pub ra: u64,
    pub sp: u64,

    // callee-saved
    pub s0: u64,
    pub s1: u64,
    pub s2: u64,
    pub s3: u64,
    pub s4: u64,
    pub s5: u64,
    pub s6: u64,
    pub s7: u64,
    pub s8: u64,
    pub s9: u64,
    pub s10: u64,
    pub s11: u64,
}

impl Context {
    const fn zero() -> Self {
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

    const fn clear(&mut self) {
        *self = Self::zero();
    }
}

/// Per-CPU state.
#[repr(C)]
pub struct Cpu {
    /// The process running on this Cpu, or null.
    proc: Option<NonNull<Proc>>,
    /// switch() here to enter scheduler()
    context: Context,
    /// Depth of `push_off()` nesting.
    pub noff: c_int,
    /// Were interrupts enabled before `push_off()`?
    pub intena: c_int,
}

unsafe impl Sync for Cpu {}

impl Cpu {
    const fn zero() -> Self {
        Self {
            proc: None,
            context: Context::zero(),
            noff: 0,
            intena: 0,
        }
    }

    /// Returns this CPU's cpu struct.
    ///
    /// Interrupts must be disabled.
    #[inline]
    pub fn mycpu() -> *mut Self {
        let id = cpuid();
        unsafe {
            let cpu = &mut CPUS[id as usize];
            ptr::from_mut(cpu)
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TrapFrame {
    /// Kernel page table.
    pub kernel_satp: u64, // 0
    /// Top of process's kernel stack.
    pub kernel_sp: u64, // 8
    /// Usertrap().
    pub kernel_trap: u64, // 16
    /// Saved user program counter.
    pub epc: u64, // 24
    /// saved kernel tp
    pub kernel_hartid: u64, // 32
    pub ra: u64,  // 40
    pub sp: u64,  // 48
    pub gp: u64,  // 56
    pub tp: u64,  // 64
    pub t0: u64,  // 72
    pub t1: u64,  // 80
    pub t2: u64,  // 88
    pub s0: u64,  // 96
    pub s1: u64,  // 104
    pub a0: u64,  // 112
    pub a1: u64,  // 120
    pub a2: u64,  // 128
    pub a3: u64,  // 136
    pub a4: u64,  // 144
    pub a5: u64,  // 152
    pub a6: u64,  // 160
    pub a7: u64,  // 168
    pub s2: u64,  // 176
    pub s3: u64,  // 184
    pub s4: u64,  // 192
    pub s5: u64,  // 200
    pub s6: u64,  // 208
    pub s7: u64,  // 216
    pub s8: u64,  // 224
    pub s9: u64,  // 232
    pub s10: u64, // 240
    pub s11: u64, // 248
    pub t3: u64,  // 256
    pub t4: u64,  // 264
    pub t5: u64,  // 272
    pub t6: u64,  // 280
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcState {
    Unused = 0,
    Used,
    Sleeping,
    Runnable,
    Running,
    Zombie,
}

/// Per-process state.
#[repr(C)]
pub struct Proc {
    lock: SpinLock,

    // lock must be held when using these:
    /// Process state.
    state: ProcState,
    /// If non-zero, sleeping on chan
    chan: *const c_void,
    /// If non-zero, have been killed
    killed: c_int,
    /// Exit status to be returned to parent's wait
    xstate: c_int,
    /// Process ID
    pid: ProcId,

    // wait_lock must be held when using this
    /// Parent process
    parent: Option<NonNull<Proc>>,

    // these are private to the process, so lock need not be held.
    /// VIrtual address of kernel stack.
    kstack: usize,
    /// Size of process memory (bytes).
    sz: usize,
    /// User page table,
    pagetable: Option<NonNull<PageTable>>,
    /// Data page for trampoline.S
    trapframe: Option<NonNull<TrapFrame>>,
    /// switch() here to run process
    context: Context,
    /// Open files
    ofile: [Option<NonNull<File>>; NOFILE],
    /// CUrrent directory
    cwd: Option<NonNull<Inode>>,
    // Process name (debugging)
    name: [c_char; 16],
}

unsafe impl Sync for Proc {}

impl Proc {
    const fn new() -> Self {
        Self {
            lock: SpinLock::new(c"proc"),
            state: ProcState::Unused,
            chan: ptr::null(),
            killed: 0,
            xstate: 0,
            pid: ProcId(0),
            parent: None,
            kstack: 0,
            sz: 0,
            pagetable: None,
            trapframe: None,
            context: Context::zero(),
            ofile: [None; NOFILE],
            cwd: None,
            name: [0; 16],
        }
    }

    /// Returns the current process.
    pub fn myproc() -> Option<&'static mut Self> {
        spinlock::push_off();
        let c = Cpu::mycpu();
        let p = unsafe { (*c).proc };
        spinlock::pop_off();
        p.map(|mut p| unsafe { p.as_mut() })
    }

    pub fn pid(&self) -> ProcId {
        self.pid
    }

    pub fn name(&self) -> &str {
        unsafe { CStr::from_ptr(self.name.as_ptr()).to_str().unwrap() }
    }

    pub fn kstack(&self) -> usize {
        self.kstack
    }

    pub fn pagetable(&self) -> Option<&PageTable> {
        self.pagetable.map(|pt| unsafe { pt.as_ref() })
    }

    fn pagetable_mut(&mut self) -> Option<&mut PageTable> {
        self.pagetable.map(|mut pt| unsafe { pt.as_mut() })
    }

    pub fn trapframe(&self) -> Option<&TrapFrame> {
        self.trapframe.map(|tf| unsafe { tf.as_ref() })
    }

    pub fn trapframe_mut(&mut self) -> Option<&mut TrapFrame> {
        self.trapframe.map(|mut tf| unsafe { tf.as_mut() })
    }

    fn parent(&self) -> Option<&Self> {
        assert!(WAIT_LOCK.holding());
        self.parent.map(|p| unsafe { p.as_ref() })
    }

    fn parent_mut(&mut self) -> Option<&mut Self> {
        assert!(WAIT_LOCK.holding());
        self.parent.map(|mut p| unsafe { p.as_mut() })
    }

    fn allocate_pid() -> ProcId {
        static NEXT_PID: AtomicI32 = AtomicI32::new(1);
        let pid = NEXT_PID.fetch_add(1, Ordering::Relaxed);
        ProcId(pid)
    }

    /// Returns UNUSED proc in the process table.
    ///
    /// If there is no UNUSED proc, returns None.
    /// This function also locks the proc.
    fn lock_unused_proc() -> Option<&'static mut Self> {
        for p in unsafe { (&raw mut PROC).as_mut().unwrap() } {
            p.lock.acquire();
            if p.state != ProcState::Unused {
                p.lock.release();
                continue;
            }
            return Some(p);
        }
        None
    }

    /// Returns a new process.
    ///
    /// Locks in the process table for an UNUSED proc.
    /// If found, initialize state required to run in the kenrnel,
    /// and return with the lock held.
    /// If there are no free procs, return None.
    fn allocate() -> Option<&'static mut Self> {
        let p = Self::lock_unused_proc()?;

        p.pid = Self::allocate_pid();
        p.state = ProcState::Used;

        let res: Result<(), ()> = (|| {
            p.pid = Self::allocate_pid();
            p.state = ProcState::Used;

            // Allocate a trapframe page.
            p.trapframe = Some(kalloc::alloc_page().ok_or(())?.cast());
            // An empty user page table.
            p.pagetable = Some(create_pagetable(p).ok_or(())?);
            // Set up new context to start executing ad forkret,
            // which returns to user space.
            p.context.clear();
            p.context.ra = forkret as usize as u64;
            p.context.sp = (p.kstack + PAGE_SIZE) as u64;
            Ok(())
        })();

        if res.is_err() {
            p.free();
            p.lock.release();
            return None;
        }

        Some(p)
    }

    /// Frees a proc structure and the data hangind from it,
    /// including user pages.
    ///
    /// p.lock must be held.
    fn free(&mut self) {
        assert!(self.lock.holding());

        if let Some(tf) = self.trapframe.take() {
            kalloc::free_page(tf.cast());
        }
        if let Some(pt) = self.pagetable.take() {
            free_pagetable(pt, self.sz);
        }
        self.sz = 0;
        self.pid = ProcId(0);
        self.parent = None;
        self.name.fill(0);
        self.chan = ptr::null();
        self.killed = 0;
        self.xstate = 0;
        self.state = ProcState::Unused;
    }

    pub fn set_killed(&mut self) {
        self.lock.acquire();
        self.killed = 1;
        self.lock.release();
    }

    pub fn killed(&self) -> bool {
        self.lock.acquire();
        let k = self.killed != 0;
        self.lock.release();
        k
    }

    pub fn is_valid_addr(&self, addr_range: Range<VirtAddr>) -> bool {
        let end = VirtAddr::new(self.sz);
        addr_range.start < end && addr_range.end <= end // both tests needed, in case of overflow
    }
}

/// ALlocates a page for each process's kernel stack.
///
/// Map it high in memory, followed by an invalid
/// guard page.
pub fn map_stacks(kpgtbl: &mut PageTable) {
    unsafe {
        let proc = (&raw mut PROC).as_mut().unwrap();
        for (i, _p) in proc.iter_mut().enumerate() {
            let pa = kalloc::alloc_page().unwrap();
            let va = kstack(i);
            kpgtbl
                .map_page(
                    VirtAddr::new(va),
                    PhysAddr::new(pa.addr().get()),
                    PtEntryFlags::RW,
                )
                .unwrap();
        }
    }
}

/// Initialize the proc table.
pub fn init() {
    unsafe {
        let proc = (&raw mut PROC).as_mut().unwrap();
        for (i, p) in proc.iter_mut().enumerate() {
            p.kstack = kstack(i)
        }
    }
}

/// Returns current CPU's ID.
///
/// Must be called with interrupts disabled,
/// to prevent race with process being moved
/// to a different CPU.
pub fn cpuid() -> i32 {
    let id: u64;
    unsafe { asm!("mv {}, tp", out(reg) id) };
    id as i32
}

/// Creates a user page table for a given process, with no user memory,
/// but with trampoline and trapframe pages.
fn create_pagetable(p: &mut Proc) -> Option<NonNull<PageTable>> {
    // An empty page table.
    let mut pagetable_ptr = vm::user::create().ok()?;
    let pagetable = unsafe { pagetable_ptr.as_mut() };

    // map the trampoline code (for system call return)
    // at the highest user virtual address.
    // only the supervisort uses it, on the way
    // to/from user space, so no PtEntryFlags::U
    if pagetable
        .map_page(
            TRAMPOLINE,
            PhysAddr::new(trampoline::trampoline as usize),
            PtEntryFlags::RX,
        )
        .is_err()
    {
        let _ = pagetable; // drop pagetable reference
        unsafe {
            vm::user::free(pagetable_ptr.addr().get(), 0);
        }
        return None;
    }

    // map the trapframe page just below the trampoline page, for
    // trampoline.S.
    if pagetable
        .map_page(
            TRAPFRAME,
            PhysAddr::new(p.trapframe.map(|tf| tf.addr().get()).unwrap_or(0)),
            PtEntryFlags::RW,
        )
        .is_err()
    {
        vm::user::unmap(pagetable, TRAMPOLINE, 1, false);
        let _ = pagetable; // drop pagetable reference
        unsafe {
            vm::user::free(pagetable_ptr.addr().get(), 0);
        }
        return None;
    }

    Some(pagetable_ptr)
}

/// Frees a process's page table, and free the
/// physical memory it refers to.
fn free_pagetable(mut pagetable_ptr: NonNull<PageTable>, sz: usize) {
    let pagetable = unsafe { pagetable_ptr.as_mut() };
    vm::user::unmap(pagetable, TRAMPOLINE, 1, false);
    vm::user::unmap(pagetable, TRAPFRAME, 1, false);
    let _ = pagetable; // drop pagetable reference
    unsafe {
        vm::user::free(pagetable_ptr.addr().get(), sz);
    }
}

/// A user program that calls `exec("/init")`.
///
/// Assembled from "user/initcode.S".
/// `od -t xC user/initcode`
static INIT_CODE: [u8; 52] = [
    0x17, 0x05, 0x00, 0x00, 0x13, 0x05, 0x45, 0x02, 0x97, 0x05, 0x00, 0x00, 0x93, 0x85, 0x35, 0x02,
    0x93, 0x08, 0x70, 0x00, 0x73, 0x00, 0x00, 0x00, 0x93, 0x08, 0x20, 0x00, 0x73, 0x00, 0x00, 0x00,
    0xef, 0xf0, 0x9f, 0xff, 0x2f, 0x69, 0x6e, 0x69, 0x74, 0x00, 0x00, 0x24, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00,
];

/// Set up first user process.
pub fn user_init() {
    let p = Proc::allocate().unwrap();
    unsafe {
        INITPROC = Some(NonNull::from_mut(p));
    }

    // allocate one user page and copy initcode's instructions
    // and data into it.
    vm::user::map_first(p.pagetable_mut().unwrap(), &INIT_CODE);
    p.sz = PAGE_SIZE;

    // prepare for the very first `return` from kernel to user.
    let trapframe = p.trapframe_mut().unwrap();
    trapframe.epc = 0; // user program counter
    trapframe.sp = PAGE_SIZE as u64; // user stack pointer

    p.name = *b"initcode\0\0\0\0\0\0\0\0";
    p.cwd = fs::namei(c"/");

    p.state = ProcState::Runnable;

    p.lock.release();
}

/// Grows or shrink user memory by nBytes.
fn grow_proc(n: isize) -> Result<(), ()> {
    let p = Proc::myproc().unwrap();

    let old_sz = p.sz;
    let new_sz = (p.sz as isize + n) as usize;
    let pagetable = p.pagetable_mut().unwrap();

    p.sz = match new_sz.cmp(&old_sz) {
        cmp::Ordering::Equal => old_sz,
        cmp::Ordering::Less => vm::user::dealloc(pagetable, old_sz, new_sz),
        cmp::Ordering::Greater => vm::user::alloc(pagetable, old_sz, new_sz, PtEntryFlags::W)?,
    };

    Ok(())
}

/// Creates a new process, copying the parent.
///
/// Sets up child kernel stack to return as if from `fork()` system call.
fn fork() -> Option<ProcId> {
    // reborrow to prevent accidental update
    let p = &*Proc::myproc().unwrap();

    // Allocate process.
    let np = Proc::allocate()?;

    // Copy use memory from parent to child.
    if vm::user::copy(p.pagetable().unwrap(), np.pagetable_mut().unwrap(), p.sz).is_err() {
        np.free();
        np.lock.release();
        return None;
    }
    np.sz = p.sz;

    // Copy saved user registers.
    *np.trapframe_mut().unwrap() = *p.trapframe().unwrap();

    // Cause fork to return 0 in the child.
    np.trapframe_mut().unwrap().a0 = 0;

    // increment refereence counts on open file descriptors.
    for (of, nof) in p.ofile.iter().zip(&mut np.ofile) {
        if let Some(of) = of {
            *nof = file::dup(*of);
        }
    }
    np.cwd = fs::inode_dup(p.cwd.unwrap());

    np.name = p.name;

    let pid = np.pid;
    np.lock.release();

    WAIT_LOCK.acquire();
    np.parent = Some(NonNull::from_ref(p));
    WAIT_LOCK.release();

    np.lock.acquire();
    np.state = ProcState::Runnable;
    np.lock.release();

    Some(pid)
}

/// Pass p's abandoned children to init.
///
/// Caller must hold `WAIT_LOCK`
fn reparent(p: &Proc) {
    assert!(WAIT_LOCK.holding());

    for pp in unsafe { (&raw mut PROC).as_mut().unwrap() } {
        let Some(parent) = pp.parent_mut() else {
            continue;
        };
        if ptr::eq(p, parent) {
            pp.parent = unsafe { INITPROC };
            wakeup(unsafe { INITPROC }.unwrap().as_ptr().cast());
        }
    }
}

/// Exits the current process.
///
/// Does not return.
/// An exited process remains in the zombie state
/// until its parent calls `wait()`.
pub fn exit(status: i32) -> ! {
    // Ensure all destruction is done before `sched().`
    {
        let p = Proc::myproc().unwrap();

        assert!(
            !ptr::eq(p, unsafe { INITPROC }.unwrap().as_ptr()),
            "init exiting"
        );

        // Close all open files.
        for of in &mut p.ofile {
            if let Some(of) = of.take() {
                file::close(of);
            }
        }

        log::begin_op();
        fs::inode_put(p.cwd.unwrap());
        log::end_op();
        p.cwd = None;

        WAIT_LOCK.acquire();

        // Give any children to init.
        reparent(p);

        // Parent might be sleeping in wait().
        wakeup(p.parent.unwrap().as_ptr().cast());

        p.lock.acquire();
        p.xstate = status;
        p.state = ProcState::Zombie;

        WAIT_LOCK.release();
    }

    // Jump into the scheduler, never to return.
    sched();

    unreachable!("zombie exit");
}

/// Waits for a child process to exit and return its pid.
///
/// Returns `Err` if this process has no children.
fn wait(addr: VirtAddr) -> Result<ProcId, ()> {
    let p = Proc::myproc().unwrap();

    WAIT_LOCK.acquire();

    loop {
        let mut have_kids = false;
        for pp in unsafe { (&raw mut PROC).as_mut().unwrap() } {
            let Some(parent) = pp.parent() else {
                continue;
            };
            if !ptr::eq(parent, p) {
                continue;
            }

            // Make sure the child isn't still in `exit()` or `switch()``.
            pp.lock.acquire();

            have_kids = true;
            if pp.state == ProcState::Zombie {
                // Found one.
                let pid = pp.pid;
                if addr.addr() != 0
                    && vm::copy_out(p.pagetable().unwrap(), addr, &pp.xstate.to_ne_bytes()).is_err()
                {
                    pp.lock.release();
                    WAIT_LOCK.release();
                    return Err(());
                }
                pp.free();
                pp.lock.release();
                WAIT_LOCK.release();
                return Ok(pid);
            }
            pp.lock.release();
        }

        // No point waiting if we don't have any children.
        if !have_kids || p.killed() {
            WAIT_LOCK.release();
            return Err(());
        }

        // Wait for a child to exit.
        sleep_raw(ptr::from_mut(p).cast(), &WAIT_LOCK);
    }
}

/// Per-CPU process scheduler.
///
/// Each CPU calls `scheduler()` after setting itself up.
/// Scheduler never returns.
///
/// It loops doing:
///
/// - choose a process to run.
/// - switch to start running that process.
/// - eventually that process transfers control
///   via switch back to the scheduler.
pub fn scheduler() -> ! {
    let cpu = Cpu::mycpu();

    unsafe {
        (*cpu).proc = None;
    }

    loop {
        // THe most recent process to run may have had interrupts
        // turned off; enable them to avoid a deadlock if all
        // processes are waiting.
        unsafe {
            sstatus::set_sie();
        }

        let mut found = false;
        for p in unsafe { (&raw mut PROC).as_mut().unwrap() } {
            p.lock.acquire();
            if p.state != ProcState::Runnable {
                p.lock.release();
                continue;
            }

            // Switch to chosen process. It is the process's job
            // to release its lock and then reacquire it
            // before jumping back to us.
            p.state = ProcState::Running;
            unsafe {
                (*cpu).proc = Some(NonNull::from_mut(p));
                switch::switch(&mut (*cpu).context, &mut p.context);
            }

            // Process is done running for now.
            // It should have changed its p->state before coming back.
            unsafe {
                (*cpu).proc = None;
                found = true;
            }
            p.lock.release();
        }

        if !found {
            unsafe {
                sstatus::clear_sie();
                asm!("wfi");
            }
        }
    }
}

/// Switch to shcduler.
///
/// Must hold only `Proc::lock` and  have changed `proc->state`.
///
/// Saves and restores `Cpu:intena` because `inteta` is a property of this kernel thread,
/// not this CPU. It should be `Proc::intena` and `Proc::noff`, but that would break in the
/// fea places where a lock is held but
/// there's no process.
fn sched() {
    let p = Proc::myproc().unwrap();

    assert!(p.lock.holding());
    assert_eq!(unsafe { Cpu::mycpu().as_ref().unwrap() }.noff, 1);
    assert_ne!(p.state, ProcState::Running);
    assert!(!sstatus::read().sie());

    let intena = unsafe { Cpu::mycpu().as_ref().unwrap() }.intena;
    unsafe {
        switch::switch(&mut p.context, &mut (*Cpu::mycpu()).context);
    }
    unsafe { Cpu::mycpu().as_mut().unwrap() }.intena = intena;
}

/// Gives up the CPU for one shceduling round.
pub fn yield_() {
    if let Some(p) = Proc::myproc() {
        p.lock.acquire();
        p.state = ProcState::Runnable;
        sched();
        p.lock.release();
    }
}

/// A fork child's very first scheduling by `scheduler()`
/// will switch for forkret.
extern "C" fn forkret() {
    static FIRST: AtomicBool = AtomicBool::new(true);

    // Still holding `p->lock` from `fork()`.
    Proc::myproc().unwrap().lock.release();

    if FIRST.load(Ordering::Acquire) {
        // File system initialization must be run in the context of a
        // regular process (e.g., because it calls sleep), and thus cannot
        // be run from main().
        fs::init(ROOTDEV);

        FIRST.store(false, Ordering::Release);
    }

    trap::trap_user_ret();
}

/// Automatically releases `lock` and sleeps on `chan``.
///
/// Reacquires lock when awakened.
pub fn sleep<T>(chan: *const c_void, lock: &mut MutexGuard<T>) {
    unsafe { sleep_raw(chan, lock.spinlock()) }
}

/// Automatically releases `lock` and sleeps on `chan``.
///
/// Reacquires lock when awakened.
fn sleep_raw(chan: *const c_void, lock: &SpinLock) {
    let p = Proc::myproc().unwrap();

    // Must acquire `p.lock` in order to change
    // `p.state` and then call `sched()`.
    // Once we hold `p.lock()`, we can be
    // guaranteed that we won't miss any wakeup
    // (wakeup locks `p.lock`),
    // so it's okay to release `lock' here.`
    p.lock.acquire();
    lock.release();

    // Go to sleep.
    p.chan = chan;
    p.state = ProcState::Sleeping;

    sched();

    // Tidy up.
    p.chan = ptr::null();

    // Reacquire original lock.
    p.lock.release();
    lock.acquire();
}

/// Wakes up all processes sleeping on `chan`.
///
/// Must be called without any processes locked.
pub fn wakeup(chan: *const c_void) {
    let myproc = Proc::myproc()
        .map(|p| ptr::from_ref(p))
        .unwrap_or(ptr::null());
    for p in unsafe { (&raw mut PROC).as_mut().unwrap() } {
        if ptr::eq(p, myproc) {
            continue;
        }

        p.lock.acquire();
        if p.state == ProcState::Sleeping && p.chan == chan {
            p.state = ProcState::Runnable;
        }
        p.lock.release();
    }
}

/// Kills the process with the given PID.
///
/// The victim won't exit until it tries to return
/// to user spaec (see `usertrap()`).
fn kill(pid: ProcId) -> Result<(), ()> {
    for p in unsafe { (&raw mut PROC).as_mut().unwrap() } {
        p.lock.acquire();
        if p.pid == pid {
            p.killed = 1;
            if p.state == ProcState::Sleeping {
                // Wake process from sleep().
                p.state = ProcState::Runnable;
            }
            p.lock.release();
            return Ok(());
        }
        p.lock.release();
    }
    Err(())
}

/// Copies to either a user address, or kernel address,
/// depending on `user_dst`.
pub fn either_copy_out(user_dst: bool, dst: usize, src: &[u8]) -> Result<(), ()> {
    let p = Proc::myproc().unwrap();
    if user_dst {
        return vm::copy_out(p.pagetable().unwrap(), VirtAddr::new(dst), src);
    }

    unsafe {
        let dst = ptr::without_provenance_mut::<u8>(dst);
        let dst = slice::from_raw_parts_mut(dst, src.len());
        dst.copy_from_slice(src);
        Ok(())
    }
}

/// Copies from either a user address, or kernel address,
/// depending on `user_src`.
pub fn either_copy_in(dst: &mut [u8], user_src: bool, src: usize) -> Result<(), ()> {
    let p = Proc::myproc().unwrap();
    if user_src {
        return vm::copy_in(p.pagetable().unwrap(), dst, VirtAddr::new(src));
    }
    unsafe {
        let src = ptr::without_provenance::<u8>(src);
        let src = slice::from_raw_parts(src, dst.len());
        dst.copy_from_slice(src);
        Ok(())
    }
}

/// Prints a process listing to console.
///
/// For debugging.
/// Runs when user type ^P on console
///
/// No lock to avoid wedging a stuck machine further.
pub fn dump() {
    println!();
    for p in unsafe { (&raw mut PROC).as_mut().unwrap() } {
        if p.state == ProcState::Unused {
            continue;
        }

        let state = match p.state {
            ProcState::Unused => "unused",
            ProcState::Used => "used",
            ProcState::Sleeping => "sleep",
            ProcState::Runnable => "runble",
            ProcState::Running => "run",
            ProcState::Zombie => "zombie",
        };
        let name = CStr::from_bytes_until_nul(&p.name)
            .unwrap()
            .to_str()
            .unwrap();

        println!(
            "{pid} {state:<10} {name}",
            pid = p.pid.0,
            state = state,
            name = name
        );
    }
}
