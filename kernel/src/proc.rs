use core::{
    arch::asm,
    cell::UnsafeCell,
    cmp,
    ffi::{CStr, c_char, c_int, c_void},
    fmt, mem,
    ops::Range,
    ptr::{self, NonNull},
    slice,
    sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, Ordering},
};

use crate::{
    cpu::Cpu,
    file::{self, File, Inode},
    fs, interrupt, kalloc, log,
    memlayout::{TRAMPOLINE, TRAPFRAME, kstack},
    param::{NOFILE, NPROC, ROOT_DEV},
    println, switch,
    sync::{RawSpinLock, SpinLockGuard},
    trampoline, trap,
    vm::{self, PAGE_SIZE, PageTable, PhysAddr, PtEntryFlags, VirtAddr},
};

static PROC: [Proc; NPROC] = [const { Proc::new() }; NPROC];
static INITPROC: AtomicPtr<Proc> = AtomicPtr::new(ptr::null_mut());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct ProcId(i32);

impl fmt::Display for ProcId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl ProcId {
    pub const fn new(pid: i32) -> Self {
        Self(pid)
    }

    pub fn get(&self) -> i32 {
        self.0
    }
}

/// Helps ensure that wakeups of wait()ing
/// parents are not lost.
///
/// Helps obey the memory model when using `Proc::parent`.
/// Must be acquired before any `Proc::lock`.
static WAIT_LOCK: RawSpinLock = RawSpinLock::new();

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
    pub const fn zeroed() -> Self {
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
        *self = Self::zeroed();
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
    lock: RawSpinLock,

    // lock must be held when using these:
    /// Process state.
    state: UnsafeCell<ProcState>,
    /// If non-zero, sleeping on chan
    chan: UnsafeCell<*const c_void>,
    /// If non-zero, have been killed
    killed: UnsafeCell<c_int>,
    /// Exit status to be returned to parent's wait
    xstate: UnsafeCell<c_int>,
    /// Process ID
    pid: UnsafeCell<ProcId>,

    // wait_lock must be held when using this
    /// Parent process
    parent: UnsafeCell<Option<NonNull<Proc>>>,

    // these are private to the process, so lock need not be held.
    /// Virtual address of kernel stack.
    kstack: UnsafeCell<usize>,
    /// Size of process memory (bytes).
    sz: UnsafeCell<usize>,
    /// User page table,
    pagetable: UnsafeCell<Option<NonNull<PageTable>>>,
    /// Data page for trampoline.S
    trapframe: UnsafeCell<Option<NonNull<TrapFrame>>>,
    /// switch() here to run process
    context: UnsafeCell<Context>,
    /// Open files
    ofile: [UnsafeCell<Option<NonNull<File>>>; NOFILE],
    /// CUrrent directory
    cwd: UnsafeCell<Option<NonNull<Inode>>>,
    // Process name (debugging)
    name: UnsafeCell<[c_char; 16]>,
}

unsafe impl Sync for Proc {}

impl Proc {
    const fn new() -> Self {
        Self {
            lock: RawSpinLock::new(),
            state: UnsafeCell::new(ProcState::Unused),
            chan: UnsafeCell::new(ptr::null()),
            killed: UnsafeCell::new(0),
            xstate: UnsafeCell::new(0),
            pid: UnsafeCell::new(ProcId(0)),
            parent: UnsafeCell::new(None),
            kstack: UnsafeCell::new(0),
            sz: UnsafeCell::new(0),
            pagetable: UnsafeCell::new(None),
            trapframe: UnsafeCell::new(None),
            context: UnsafeCell::new(Context::zeroed()),
            ofile: [const { UnsafeCell::new(None) }; NOFILE],
            cwd: UnsafeCell::new(None),
            name: UnsafeCell::new([0; 16]),
        }
    }

    /// Returns the current process.
    pub fn current() -> &'static Self {
        Self::try_current().unwrap()
    }

    /// Returns the current process.
    pub fn try_current() -> Option<&'static Self> {
        let p = interrupt::with_push_disabled(|| {
            let c = Cpu::current();
            unsafe { *c.proc.get() }
        });

        p.map(|p| unsafe { p.as_ref() })
    }

    pub fn pid(&self) -> ProcId {
        unsafe { *self.pid.get() }
    }

    pub fn name(&self) -> &str {
        unsafe {
            CStr::from_ptr((*self.name.get()).as_ptr())
                .to_str()
                .unwrap()
        }
    }

    pub fn cwd(&self) -> Option<NonNull<Inode>> {
        unsafe { *self.cwd.get() }
    }

    pub fn name_mut(&self) -> NonNull<[u8; 16]> {
        NonNull::new(self.name.get()).unwrap()
    }

    pub fn size(&self) -> usize {
        unsafe { *self.sz.get() }
    }

    pub fn kstack(&self) -> usize {
        unsafe { *self.kstack.get() }
    }

    pub fn pagetable(&self) -> Option<&PageTable> {
        unsafe { *self.pagetable.get() }.map(|pt| unsafe { pt.as_ref() })
    }

    fn pagetable_mut(&self) -> Option<&mut PageTable> {
        unsafe { *self.pagetable.get() }.map(|mut pt| unsafe { pt.as_mut() })
    }

    pub fn update_pagetable(&self, pagetable: NonNull<PageTable>, sz: usize) {
        let old_pt = unsafe { ptr::replace(self.pagetable.get(), Some(pagetable)) };
        let old_sz = unsafe { ptr::replace(self.sz.get(), sz) };
        if let Some(old) = old_pt {
            free_pagetable(old, old_sz);
        }
    }

    pub fn trapframe(&self) -> Option<&TrapFrame> {
        unsafe { *self.trapframe.get() }.map(|tf| unsafe { tf.as_ref() })
    }

    pub fn trapframe_mut(&self) -> Option<&mut TrapFrame> {
        unsafe { *self.trapframe.get() }.map(|mut tf| unsafe { tf.as_mut() })
    }

    fn parent(&self) -> Option<&Self> {
        assert!(WAIT_LOCK.holding());
        unsafe { *self.parent.get() }.map(|p| unsafe { p.as_ref() })
    }

    fn parent_mut(&self) -> Option<&mut Self> {
        assert!(WAIT_LOCK.holding());
        unsafe { *self.parent.get() }.map(|mut p| unsafe { p.as_mut() })
    }

    pub fn ofile(&self, fd: usize) -> Option<NonNull<File>> {
        unsafe { *self.ofile.get(fd)?.get() }
    }

    pub fn add_ofile(&self, file: NonNull<File>) -> Option<usize> {
        for (i, of) in self.ofile.iter().enumerate() {
            if unsafe { *of.get() }.is_none() {
                unsafe {
                    *of.get() = Some(file);
                }
                return Some(i);
            }
        }
        None
    }

    pub fn unset_ofile(&self, fd: usize) {
        unsafe {
            *self.ofile.get(fd).unwrap().get() = None;
        }
    }

    pub fn update_cwd(&self, cwd: NonNull<Inode>) -> NonNull<Inode> {
        unsafe { mem::replace(&mut *self.cwd.get(), Some(cwd)) }.unwrap()
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
    fn lock_unused_proc() -> Option<&'static Self> {
        for p in &PROC {
            p.lock.acquire();
            if unsafe { *p.state.get() } != ProcState::Unused {
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
    fn allocate() -> Option<&'static Self> {
        let p = Self::lock_unused_proc()?;
        assert!(p.lock.holding());

        unsafe {
            *p.pid.get() = Self::allocate_pid();
            *p.state.get() = ProcState::Used;
        }

        let res: Result<(), ()> = (|| {
            unsafe {
                *p.pid.get() = Self::allocate_pid();
                *p.state.get() = ProcState::Used;

                // Allocate a trapframe page.
                *p.trapframe.get() = Some(kalloc::alloc_page().ok_or(())?.cast());
                // An empty user page table.
                *p.pagetable.get() = Some(create_pagetable(p).ok_or(())?);
                // Set up new context to start executing ad forkret,
                // which returns to user space.
                (*p.context.get()).clear();
                (*p.context.get()).ra = forkret as usize as u64;
                (*p.context.get()).sp = ((*p.kstack.get()) + PAGE_SIZE) as u64;
            }
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
    fn free(&self) {
        assert!(self.lock.holding());

        if let Some(tf) = unsafe { *self.trapframe.get() }.take() {
            kalloc::free_page(tf.cast());
        }
        if let Some(pt) = unsafe { *self.pagetable.get() }.take() {
            free_pagetable(pt, unsafe { *self.sz.get() });
        }
        unsafe {
            *self.sz.get() = 0;
            *self.pid.get() = ProcId(0);
            *self.parent.get() = None;
            (*self.name.get()).fill(0);
            *self.chan.get() = ptr::null();
            *self.killed.get() = 0;
            *self.xstate.get() = 0;
            *self.state.get() = ProcState::Unused;
        }
    }

    pub fn set_killed(&self) {
        self.lock.acquire();
        unsafe {
            *self.killed.get() = 1;
        }
        self.lock.release();
    }

    pub fn killed(&self) -> bool {
        self.lock.acquire();
        let k = unsafe { *self.killed.get() } != 0;
        self.lock.release();
        k
    }

    pub fn is_valid_addr(&self, addr_range: Range<VirtAddr>) -> bool {
        let end = VirtAddr::new(unsafe { *self.sz.get() });
        addr_range.start < end && addr_range.end <= end // both tests needed, in case of overflow
    }
}

/// ALlocates a page for each process's kernel stack.
///
/// Map it high in memory, followed by an invalid
/// guard page.
pub fn map_stacks(kpgtbl: &mut PageTable) {
    for (i, _p) in PROC.iter().enumerate() {
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

/// Initialize the proc table.
pub fn init() {
    unsafe {
        for (i, p) in PROC.iter().enumerate() {
            *p.kstack.get() = kstack(i);
        }
    }
}

/// Creates a user page table for a given process, with no user memory,
/// but with trampoline and trapframe pages.
pub fn create_pagetable(p: &Proc) -> Option<NonNull<PageTable>> {
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
            PhysAddr::new(
                unsafe { *p.trapframe.get() }
                    .map(|tf| tf.addr().get())
                    .unwrap_or(0),
            ),
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
pub fn free_pagetable(mut pagetable_ptr: NonNull<PageTable>, sz: usize) {
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
    INITPROC.store(ptr::from_ref(p).cast_mut(), Ordering::Release);

    // allocate one user page and copy initcode's instructions
    // and data into it.
    vm::user::map_first(p.pagetable_mut().unwrap(), &INIT_CODE);
    unsafe {
        *p.sz.get() = PAGE_SIZE;
    }

    // prepare for the very first `return` from kernel to user.
    let trapframe = p.trapframe_mut().unwrap();
    trapframe.epc = 0; // user program counter
    trapframe.sp = PAGE_SIZE as u64; // user stack pointer

    unsafe {
        *p.name.get() = *b"initcode\0\0\0\0\0\0\0\0";
        *p.cwd.get() = Some(fs::resolve_path(p, b"/").unwrap());
        *p.state.get() = ProcState::Runnable;
    }

    p.lock.release();
}

/// Grows or shrink user memory by nBytes.
pub fn grow_proc(p: &Proc, n: isize) -> Result<(), ()> {
    let old_sz = unsafe { *p.sz.get() };
    let new_sz = (old_sz as isize + n) as usize;
    let pagetable = p.pagetable_mut().unwrap();

    unsafe {
        *p.sz.get() = match new_sz.cmp(&old_sz) {
            cmp::Ordering::Equal => old_sz,
            cmp::Ordering::Less => vm::user::dealloc(pagetable, old_sz, new_sz),
            cmp::Ordering::Greater => vm::user::alloc(pagetable, old_sz, new_sz, PtEntryFlags::W)?,
        }
    };

    Ok(())
}

/// Creates a new process, copying the parent.
///
/// Sets up child kernel stack to return as if from `fork()` system call.
pub fn fork(p: &Proc) -> Option<ProcId> {
    // Allocate process.
    let np = Proc::allocate()?;

    // Copy use memory from parent to child.
    if vm::user::copy(
        p.pagetable().unwrap(),
        np.pagetable_mut().unwrap(),
        unsafe { *p.sz.get() },
    )
    .is_err()
    {
        np.free();
        np.lock.release();
        return None;
    }
    unsafe {
        *np.sz.get() = *p.sz.get();
    }

    // Copy saved user registers.
    *np.trapframe_mut().unwrap() = *p.trapframe().unwrap();

    // Cause fork to return 0 in the child.
    np.trapframe_mut().unwrap().a0 = 0;

    // increment refereence counts on open file descriptors.
    for (of, nof) in p.ofile.iter().zip(&np.ofile) {
        if let Some(of) = unsafe { *of.get() } {
            unsafe {
                *nof.get() = Some(file::dup(of.as_ref()).into());
            }
        }
    }
    unsafe {
        *np.cwd.get() = Some(fs::inode_dup((*p.cwd.get()).unwrap()));
        *np.name.get() = *p.name.get();
    }

    let pid = unsafe { *np.pid.get() };
    np.lock.release();

    WAIT_LOCK.acquire();
    unsafe {
        *np.parent.get() = Some(p.into());
    }
    WAIT_LOCK.release();

    np.lock.acquire();
    unsafe {
        *np.state.get() = ProcState::Runnable;
    }
    np.lock.release();

    Some(pid)
}

/// Pass p's abandoned children to init.
///
/// Caller must hold `WAIT_LOCK`
fn reparent(p: &Proc) {
    assert!(WAIT_LOCK.holding());

    for pp in &PROC {
        let Some(parent) = pp.parent_mut() else {
            continue;
        };
        if ptr::eq(p, parent) {
            unsafe { *pp.parent.get() = NonNull::new(INITPROC.load(Ordering::Relaxed)) };
            wakeup(INITPROC.load(Ordering::Relaxed).cast());
        }
    }
}

/// Exits the current process.
///
/// Does not return.
/// An exited process remains in the zombie state
/// until its parent calls `wait()`.
pub fn exit(p: &Proc, status: i32) -> ! {
    // Ensure all destruction is done before `sched().`
    {
        assert!(
            !ptr::eq(p, INITPROC.load(Ordering::Relaxed)),
            "init exiting"
        );

        // Close all open files.
        for of in &p.ofile {
            if let Some(of) = unsafe { &mut *of.get() }.take() {
                file::close(unsafe { of.as_ref() });
            }
            assert!(unsafe { *of.get() }.is_none());
        }

        log::begin_op();
        fs::inode_put(unsafe { *p.cwd.get() }.unwrap());
        log::end_op();
        unsafe {
            *p.cwd.get() = None;
        }

        WAIT_LOCK.acquire();

        // Give any children to init.
        reparent(p);

        // Parent might be sleeping in wait().
        wakeup(unsafe { *p.parent.get() }.unwrap().as_ptr().cast());

        p.lock.acquire();
        unsafe {
            *p.xstate.get() = status;
            *p.state.get() = ProcState::Zombie;
        }

        WAIT_LOCK.release();
    }

    // Jump into the scheduler, never to return.
    sched(p);

    unreachable!("zombie exit");
}

/// Waits for a child process to exit and return its pid.
///
/// Returns `Err` if this process has no children.
pub fn wait(p: &Proc, addr: VirtAddr) -> Result<ProcId, ()> {
    WAIT_LOCK.acquire();

    loop {
        let mut have_kids = false;
        for pp in &PROC {
            let Some(parent) = pp.parent() else {
                continue;
            };
            if !ptr::eq(parent, p) {
                continue;
            }

            // Make sure the child isn't still in `exit()` or `switch()``.
            pp.lock.acquire();

            have_kids = true;
            if unsafe { *pp.state.get() } == ProcState::Zombie {
                // Found one.
                let pid = unsafe { *pp.pid.get() };
                if addr.addr() != 0
                    && vm::copy_out(p.pagetable().unwrap(), addr, &unsafe { *pp.xstate.get() })
                        .is_err()
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
        let chan = ptr::from_ref(p).cast();
        sleep_raw(chan, &WAIT_LOCK);
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
    let cpu = Cpu::current();

    unsafe {
        *cpu.proc.get() = None;
    }

    loop {
        // The most recent process to run may have had interrupts
        // turned off; enable them to avoid a deadlock if all
        // processes are waiting.
        interrupt::enable();

        let mut found = false;
        for p in &PROC {
            p.lock.acquire();
            if unsafe { *p.state.get() } != ProcState::Runnable {
                p.lock.release();
                continue;
            }

            // Switch to chosen process. It is the process's job
            // to release its lock and then reacquire it
            // before jumping back to us.
            unsafe { *p.state.get() = ProcState::Running };
            unsafe {
                *cpu.proc.get() = Some(p.into());
                switch::switch(cpu.context.get(), p.context.get());
            }

            // Process is done running for now.
            // It should have changed its p->state before coming back.
            unsafe {
                *cpu.proc.get() = None;
                found = true;
            }
            p.lock.release();
        }

        if !found {
            unsafe {
                // nothing to run, stop running on this core until an interrupt.
                interrupt::enable();
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
/// few places where a lock is held but there's no process.
fn sched(p: &Proc) {
    assert!(p.lock.holding());
    assert_eq!(interrupt::disabled_depth(), 1);
    assert_ne!(unsafe { *p.state.get() }, ProcState::Running);
    assert!(!interrupt::is_enabled());

    let int_enabled = interrupt::is_enabled_before_push();
    switch::switch(p.context.get(), Cpu::current().context.get());
    unsafe {
        interrupt::force_set_before_push(int_enabled);
    }
}

/// Gives up the CPU for one shceduling round.
pub fn yield_(p: &Proc) {
    p.lock.acquire();
    unsafe { *p.state.get() = ProcState::Runnable };
    sched(p);
    p.lock.release();
}

/// A fork child's very first scheduling by `scheduler()`
/// will switch for forkret.
extern "C" fn forkret() {
    static FIRST: AtomicBool = AtomicBool::new(true);

    // Still holding `p->lock` from `fork()`.
    let p = Proc::current();
    p.lock.release();

    if FIRST.load(Ordering::Acquire) {
        // File system initialization must be run in the context of a
        // regular process (e.g., because it calls sleep), and thus cannot
        // be run from main().
        fs::init(ROOT_DEV);

        FIRST.store(false, Ordering::Release);
    }

    trap::trap_user_ret(p);
}

/// Automatically releases `lock` and sleeps on `chan``.
///
/// Reacquires lock when awakened.
pub fn sleep<T>(chan: *const c_void, lock: &mut SpinLockGuard<T>) {
    unsafe { sleep_raw(chan, lock.spinlock()) }
}

/// Automatically releases `lock` and sleeps on `chan``.
///
/// Reacquires lock when awakened.
pub fn sleep_raw(chan: *const c_void, lock: &RawSpinLock) {
    let p = Proc::current();
    // Must acquire `p.lock` in order to change
    // `p.state` and then call `sched()`.
    // Once we hold `p.lock()`, we can be
    // guaranteed that we won't miss any wakeup
    // (wakeup locks `p.lock`),
    // so it's okay to release `lock' here.`
    p.lock.acquire();
    lock.release();

    // Go to sleep.
    unsafe {
        *p.chan.get() = chan;
        *p.state.get() = ProcState::Sleeping;
    }

    sched(p);

    // Tidy up.
    unsafe {
        *p.chan.get() = ptr::null();
    }

    // Reacquire original lock.
    p.lock.release();
    lock.acquire();
}

/// Wakes up all processes sleeping on `chan`.
///
/// Must be called without any processes locked.
pub fn wakeup(chan: *const c_void) {
    let myproc = Proc::try_current()
        .map(ptr::from_ref)
        .unwrap_or(ptr::null());
    for p in &PROC {
        if ptr::eq(p, myproc) {
            continue;
        }

        p.lock.acquire();
        if unsafe { *p.state.get() } == ProcState::Sleeping && unsafe { *p.chan.get() } == chan {
            unsafe {
                *p.state.get() = ProcState::Runnable;
            }
        }
        p.lock.release();
    }
}

/// Kills the process with the given PID.
///
/// The victim won't exit until it tries to return
/// to user spaec (see `usertrap()`).
pub fn kill(pid: ProcId) -> Result<(), ()> {
    for p in &PROC {
        p.lock.acquire();
        unsafe {
            if *p.pid.get() == pid {
                *p.killed.get() = 1;
                if *p.state.get() == ProcState::Sleeping {
                    // Wake process from sleep().
                    *p.state.get() = ProcState::Runnable;
                }
                p.lock.release();
                return Ok(());
            }
        }
        p.lock.release();
    }
    Err(())
}

/// Copies to either a user address, or kernel address,
/// depending on `user_dst`.
pub fn either_copy_out_bytes(p: &Proc, user_dst: bool, dst: usize, src: &[u8]) -> Result<(), ()> {
    if user_dst {
        return vm::copy_out_bytes(p.pagetable().unwrap(), VirtAddr::new(dst), src);
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
pub fn either_copy_in_bytes(
    p: &Proc,
    dst: &mut [u8],
    user_src: bool,
    src: usize,
) -> Result<(), ()> {
    if user_src {
        return vm::copy_in_bytes(p.pagetable().unwrap(), dst, VirtAddr::new(src));
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
    for p in &PROC {
        if unsafe { *p.state.get() } == ProcState::Unused {
            continue;
        }

        let state = match unsafe { *p.state.get() } {
            ProcState::Unused => "unused",
            ProcState::Used => "used",
            ProcState::Sleeping => "sleep",
            ProcState::Runnable => "runble",
            ProcState::Running => "run",
            ProcState::Zombie => "zombie",
        };
        let name = CStr::from_bytes_until_nul(unsafe { &*p.name.get() })
            .unwrap()
            .to_str()
            .unwrap();

        println!(
            "{pid} {state:<10} {name}",
            pid = unsafe { *p.pid.get() }.0,
            state = state,
            name = name
        );
    }
}
