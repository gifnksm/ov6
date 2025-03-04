use core::{
    cell::UnsafeCell,
    char, cmp,
    ffi::c_void,
    fmt, mem,
    ops::Range,
    ptr::{self, NonNull},
    slice,
    sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, Ordering},
};

use arrayvec::ArrayString;

use crate::{
    cpu::Cpu,
    error::Error,
    file::File,
    fs::{self, DeviceNo, Inode},
    interrupt::{self, trampoline, trap},
    memory::{
        layout::{TRAMPOLINE, TRAPFRAME, kstack},
        page,
        vm::{self, PAGE_SIZE, PageTable, PhysAddr, PtEntryFlags, VirtAddr},
    },
    param::{NOFILE, NPROC},
    println,
    sync::{SpinLock, SpinLockGuard},
};

use self::{
    scheduler::Context,
    wait_lock::{Parent, WaitLock},
};

mod elf;
pub mod exec;
pub mod scheduler;
mod wait_lock;

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
    pub const INVALID: Self = ProcId(-1);

    pub const fn new(pid: i32) -> Self {
        Self(pid)
    }

    pub fn get(&self) -> i32 {
        self.0
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcState {
    Unused,
    Used,
    Sleeping { chan: *const c_void },
    Runnable,
    Running,
    Zombie { exit_status: i32 },
}

/// Per-process state that can be accessed from other processes.
pub struct ProcSharedData {
    /// Process ID
    pid: ProcId,
    /// Process name (for debugging)
    name: ArrayString<16>,
    /// Process State
    state: ProcState,
    /// Process is killed
    killed: bool,
    /// Process context.
    ///
    /// Call `switch()` here to enter process.
    context: Context,
}

impl ProcSharedData {
    pub fn pid(&self) -> ProcId {
        self.pid
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn set_name(&mut self, name: &[u8]) {
        self.name.clear();
        'outer: for chunk in name.utf8_chunks() {
            for ch in chunk.valid().chars() {
                if self.name.try_push(ch).is_err() {
                    break 'outer;
                }
            }
            if !chunk.invalid().is_empty()
                && self.name.try_push(char::REPLACEMENT_CHARACTER).is_err()
            {
                break 'outer;
            }
        }
    }

    pub fn kill(&mut self) {
        self.killed = true;
    }

    pub fn killed(&mut self) -> bool {
        self.killed
    }
}

pub struct ProcShared(SpinLock<ProcSharedData>);

impl ProcShared {
    const fn new() -> Self {
        Self(SpinLock::new(ProcSharedData {
            pid: ProcId::INVALID,
            name: ArrayString::new_const(),
            state: ProcState::Unused,
            killed: false,
            context: Context::zeroed(),
        }))
    }

    pub fn current() -> &'static Self {
        Self::try_current().unwrap()
    }

    pub fn try_current() -> Option<&'static Self> {
        let p = Proc::try_current()?;
        Some(&p.shared)
    }

    pub fn lock(&self) -> SpinLockGuard<ProcSharedData> {
        self.0.lock()
    }

    pub fn try_lock(&self) -> Result<SpinLockGuard<ProcSharedData>, Error> {
        self.0.try_lock()
    }

    unsafe fn remember_locked(&self) -> SpinLockGuard<ProcSharedData> {
        unsafe { self.0.remember_locked() }
    }
}

pub struct ProcPrivateData {
    /// Virtual address of kernel stack.
    kstack: usize,
    /// Size of process memory (bytes).
    sz: usize,
    /// User page table,
    pagetable: Option<NonNull<PageTable>>,
    /// Data page for trampoline.S
    trapframe: Option<NonNull<TrapFrame>>,
    /// Open files
    ofile: [Option<File>; NOFILE],
    /// Current directory
    cwd: Option<Inode>,
}

impl ProcPrivateData {
    const fn new() -> Self {
        Self {
            kstack: 0,
            sz: 0,
            pagetable: None,
            trapframe: None,
            ofile: [const { None }; NOFILE],
            cwd: None,
        }
    }

    pub fn kstack(&self) -> usize {
        self.kstack
    }

    pub fn size(&self) -> usize {
        self.sz
    }

    pub fn pagetable(&self) -> Option<&PageTable> {
        self.pagetable.map(|p| unsafe { p.as_ref() })
    }

    pub fn pagetable_mut(&mut self) -> Option<&mut PageTable> {
        self.pagetable.map(|mut p| unsafe { p.as_mut() })
    }

    pub fn update_pagetable(&mut self, pagetable: NonNull<PageTable>, sz: usize) {
        let old_pt = mem::replace(&mut self.pagetable, Some(pagetable));
        let old_sz = mem::replace(&mut self.sz, sz);
        if let Some(old) = old_pt {
            free_pagetable(old, old_sz);
        }
    }

    pub fn trapframe(&self) -> Option<&TrapFrame> {
        self.trapframe.map(|p| unsafe { p.as_ref() })
    }

    pub fn trapframe_mut(&mut self) -> Option<&mut TrapFrame> {
        self.trapframe.map(|mut p| unsafe { p.as_mut() })
    }

    pub fn ofile(&self, fd: usize) -> Option<&File> {
        self.ofile.get(fd).and_then(|p| p.as_ref())
    }

    pub fn add_ofile(&mut self, file: File) -> Result<usize, Error> {
        let (fd, slot) = self
            .ofile
            .iter_mut()
            .enumerate()
            .find(|(_, slot)| slot.is_none())
            .ok_or(Error::Unknown)?;
        assert!(slot.replace(file).is_none());
        Ok(fd)
    }

    pub fn unset_ofile(&mut self, fd: usize) -> Option<File> {
        self.ofile.get_mut(fd)?.take()
    }

    pub fn cwd(&self) -> Option<&Inode> {
        self.cwd.as_ref()
    }

    pub fn update_cwd(&mut self, cwd: Inode) -> Inode {
        self.cwd.replace(cwd).unwrap()
    }

    pub fn validate_addr(&self, addr_range: Range<VirtAddr>) -> Result<(), Error> {
        let end = VirtAddr::new(self.sz);
        // both tests needed, in case of overflow
        if addr_range.start < end && addr_range.end <= end {
            Ok(())
        } else {
            Err(Error::Unknown)
        }
    }
}

/// Per-process state.
pub struct Proc {
    /// Process sharead data
    shared: ProcShared,
    /// Parent process
    parent: Parent,
    /// Process private data.
    private: UnsafeCell<ProcPrivateData>,
}

unsafe impl Sync for Proc {}

impl Proc {
    const fn new() -> Self {
        Self {
            shared: ProcShared::new(),
            parent: Parent::new(),
            private: UnsafeCell::new(ProcPrivateData::new()),
        }
    }

    /// Returns the current process.
    pub fn current() -> &'static Self {
        Self::try_current().unwrap()
    }

    /// Returns the current process.
    pub fn try_current() -> Option<&'static Self> {
        let p = interrupt::with_push_disabled(|| Cpu::current().proc())?;
        Some(unsafe { p.as_ref() })
    }

    pub fn shared(&self) -> &ProcShared {
        &self.shared
    }

    #[allow(clippy::mut_from_ref)]
    pub unsafe fn private_mut(&self) -> &mut ProcPrivateData {
        unsafe { self.private.get().as_mut() }.unwrap()
    }

    fn is_child_of(&self, parent: &Self, wait_lock: &mut SpinLockGuard<WaitLock>) -> bool {
        self.parent
            .get(wait_lock)
            .map(|pp| NonNull::from(parent).eq(&pp))
            .unwrap_or(false)
    }

    fn set_parent(&self, parent: Option<NonNull<Self>>, _wait_lock: &mut SpinLockGuard<WaitLock>) {
        self.parent.set(parent, _wait_lock);
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
    fn lock_unused_proc() -> Option<(&'static Self, SpinLockGuard<'static, ProcSharedData>)> {
        for p in &PROC {
            let shared = p.shared.lock();
            if shared.state != ProcState::Unused {
                drop(shared);
                continue;
            }
            return Some((p, shared));
        }
        None
    }

    /// Returns a new process.
    ///
    /// Locks in the process table for an UNUSED proc.
    /// If found, initialize state required to run in the kenrnel,
    /// and return with the lock held.
    /// If there are no free procs, return None.
    fn allocate() -> Option<(
        &'static Self,
        SpinLockGuard<'static, ProcSharedData>,
        &'static mut ProcPrivateData,
    )> {
        let (p, mut shared) = Self::lock_unused_proc()?;

        shared.pid = Self::allocate_pid();
        shared.state = ProcState::Used;
        let private = unsafe { p.private.get().as_mut().unwrap() };

        let res: Result<(), Error> = (|| {
            // Allocate a trapframe page.
            private.trapframe = Some(page::alloc_page().ok_or(Error::Unknown)?.cast());
            // An empty user page table.
            private.pagetable = Some(create_pagetable(private).ok_or(Error::Unknown)?);
            // Set up new context to start executing ad forkret,
            // which returns to user space.
            shared.context.clear();
            shared.context.ra = forkret as usize as u64;
            shared.context.sp = (private.kstack + PAGE_SIZE) as u64;
            Ok(())
        })();

        if res.is_err() {
            p.free(private, &mut shared);
            drop(shared);
            return None;
        }

        Some((p, shared, private))
    }

    /// Frees a proc structure and the data hangind from it,
    /// including user pages.
    ///
    /// p.lock must be held.
    fn free(&self, private: &mut ProcPrivateData, shared: &mut SpinLockGuard<ProcSharedData>) {
        if let Some(tf) = private.trapframe.take() {
            unsafe {
                page::free_page(tf.cast());
            }
        }
        if let Some(pt) = private.pagetable.take() {
            free_pagetable(pt, private.sz);
        }
        private.sz = 0;
        unsafe { self.parent.reset() };
        shared.pid = ProcId::INVALID;
        shared.name.clear();
        shared.killed = false;
        shared.state = ProcState::Unused;
    }
}

/// ALlocates a page for each process's kernel stack.
///
/// Map it high in memory, followed by an invalid
/// guard page.
pub fn map_stacks(kpgtbl: &mut PageTable) {
    for (i, _p) in PROC.iter().enumerate() {
        let pa = page::alloc_page().unwrap();
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
    for (i, p) in PROC.iter().enumerate() {
        unsafe { p.private_mut() }.kstack = kstack(i);
    }
}

/// Creates a user page table for a given process, with no user memory,
/// but with trampoline and trapframe pages.
pub fn create_pagetable(private: &mut ProcPrivateData) -> Option<NonNull<PageTable>> {
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
            PhysAddr::new(private.trapframe.map(|tf| tf.addr().get()).unwrap_or(0)),
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
#[cfg(feature = "initcode_env")]
static INIT_CODE: &[u8] = const { include_bytes!(env!("INIT_CODE_PATH")) };
#[cfg(not(feature = "initcode_env"))]
static INIT_CODE: &[u8] = &[];

const _: () = const { assert!(INIT_CODE.len() < 128) };

/// Set up first user process.
pub fn user_init() {
    let (p, mut shared, private) = Proc::allocate().unwrap();
    INITPROC.store(ptr::from_ref(p).cast_mut(), Ordering::Release);

    // allocate one user page and copy initcode's instructions
    // and data into it.
    vm::user::map_first(private.pagetable_mut().unwrap(), INIT_CODE);
    private.sz = PAGE_SIZE;

    // prepare for the very first `return` from kernel to user.
    let trapframe = private.trapframe_mut().unwrap();
    trapframe.epc = 0; // user program counter
    trapframe.sp = PAGE_SIZE as u64; // user stack pointer

    let tx = fs::begin_readonly_tx();
    private.cwd = Some(Inode::from_tx(
        &fs::path::resolve(&tx, private, b"/").unwrap(),
    ));
    tx.end();
    shared.name = "initcode".try_into().unwrap();
    shared.state = ProcState::Runnable;

    drop(shared);
}

/// Grows or shrink user memory by nBytes.
pub fn grow_proc(private: &mut ProcPrivateData, n: isize) -> Result<(), Error> {
    let old_sz = private.sz;
    let new_sz = (old_sz as isize + n) as usize;
    let pagetable = private.pagetable_mut().unwrap();

    private.sz = match new_sz.cmp(&old_sz) {
        cmp::Ordering::Equal => old_sz,
        cmp::Ordering::Less => vm::user::dealloc(pagetable, old_sz, new_sz),
        cmp::Ordering::Greater => vm::user::alloc(pagetable, old_sz, new_sz, PtEntryFlags::W)?,
    };

    Ok(())
}

/// Creates a new process, copying the parent.
///
/// Sets up child kernel stack to return as if from `fork()` system call.
pub fn fork(p: &Proc, p_private: &ProcPrivateData) -> Option<ProcId> {
    let parent_name = p.shared().lock().name;

    // Allocate process.
    let (np, mut np_shared, np_private) = Proc::allocate()?;

    // Copy use memory from parent to child.
    if vm::user::copy(
        p_private.pagetable().unwrap(),
        np_private.pagetable_mut().unwrap(),
        p_private.sz,
    )
    .is_err()
    {
        np.free(np_private, &mut np_shared);
        drop(np_shared);
        return None;
    }
    np_private.sz = p_private.sz;

    // Copy saved user registers.
    *np_private.trapframe_mut().unwrap() = *p_private.trapframe().unwrap();

    // Cause fork to return 0 in the child.
    np_private.trapframe_mut().unwrap().a0 = 0;

    // increment refereence counts on open file descriptors.
    for (of, nof) in p_private.ofile.iter().zip(&mut np_private.ofile) {
        if let Some(of) = of {
            *nof = Some(of.dup());
        }
    }
    np_private.cwd = p_private.cwd.clone();
    np_shared.name = parent_name;

    let pid = np_shared.pid;
    drop(np_shared);

    let mut wait_lock = wait_lock::lock();
    np.parent.set(Some(p.into()), &mut wait_lock);
    drop(wait_lock);

    np.shared.lock().state = ProcState::Runnable;

    Some(pid)
}

/// Pass p's abandoned children to init.
///
/// Caller must hold `WAIT_LOCK`
fn reparent(p: &Proc, wait_lock: &mut SpinLockGuard<WaitLock>) {
    for pp in &PROC {
        if pp.is_child_of(p, wait_lock) {
            pp.set_parent(NonNull::new(INITPROC.load(Ordering::Relaxed)), wait_lock);
            wakeup(INITPROC.load(Ordering::Relaxed).cast());
        }
    }
}

/// Exits the current process.
///
/// Does not return.
/// An exited process remains in the zombie state
/// until its parent calls `wait()`.
pub fn exit(p: &Proc, p_private: &mut ProcPrivateData, status: i32) -> ! {
    // Ensure all destruction is done before `sched().`
    let mut shared = {
        assert!(
            !ptr::eq(p, INITPROC.load(Ordering::Relaxed)),
            "init exiting"
        );

        // Close all open files.
        for of in &mut p_private.ofile {
            if let Some(of) = of.take() {
                of.close();
            }
        }

        let tx = fs::begin_tx();
        p_private.cwd.take().unwrap().into_tx(&tx).put();
        tx.end();

        let mut wait_lock = wait_lock::lock();

        // Give any children to init.
        reparent(p, &mut wait_lock);

        // Parent might be sleeping in wait().
        wakeup(
            p.parent
                .get(&mut wait_lock)
                .map(NonNull::as_ptr)
                .unwrap_or(ptr::null_mut())
                .cast(),
        );

        let mut shared = p.shared.lock();
        shared.state = ProcState::Zombie {
            exit_status: status,
        };

        let _ = p_private; // drop mutable reference
        drop(wait_lock);
        shared
    };

    // Jump into the scheduler, never to return.
    scheduler::sched(&mut shared);

    unreachable!("zombie exit");
}

/// Waits for a child process to exit and return its pid.
///
/// Returns `Err` if this process has no children.
pub fn wait(p: &Proc, p_private: &ProcPrivateData, addr: VirtAddr) -> Result<ProcId, Error> {
    let mut wait_lock = wait_lock::lock();

    loop {
        let mut have_kids = false;
        for pp in &PROC {
            if !pp.is_child_of(p, &mut wait_lock) {
                continue;
            }

            // Make sure the child isn't still in `exit()` or `switch()``.
            let mut pp_shared = pp.shared.lock();

            have_kids = true;
            if let ProcState::Zombie { exit_status } = pp_shared.state {
                // Found one.
                // SAFETY: When in Zombie state, no other routines refer private data.
                let pp_private = unsafe { pp.private_mut() };

                let pid = pp_shared.pid;
                if addr.addr() != 0
                    && vm::copy_out(p_private.pagetable().unwrap(), addr, &exit_status).is_err()
                {
                    drop(pp_shared);
                    drop(wait_lock);
                    return Err(Error::Unknown);
                }
                pp.free(pp_private, &mut pp_shared);
                drop(pp_shared);
                drop(wait_lock);
                return Ok(pid);
            }
            drop(pp_shared);
        }

        // No point waiting if we don't have any children.
        if !have_kids || p.shared.lock().killed() {
            drop(wait_lock);
            return Err(Error::Unknown);
        }

        // Wait for a child to exit.
        let chan = ptr::from_ref(p).cast();
        wait_lock = sleep(chan, wait_lock);
    }
}

/// Gives up the CPU for one shceduling round.
pub fn yield_(p: &Proc) {
    let mut shared = p.shared.lock();
    shared.state = ProcState::Runnable;
    scheduler::sched(&mut shared);
    drop(shared);
}

/// A fork child's very first scheduling by `scheduler()`
/// will switch for forkret.
extern "C" fn forkret() {
    static FIRST: AtomicBool = AtomicBool::new(true);

    // Still holding `p->shared` from `scheduler()`.
    let p = Proc::current();
    let private = unsafe { p.private_mut() };
    let _ = unsafe { p.shared.remember_locked() }; // unlock here

    if FIRST.load(Ordering::Acquire) {
        // File system initialization must be run in the context of a
        // regular process (e.g., because it calls sleep), and thus cannot
        // be run from main().
        fs::init_in_proc(DeviceNo::ROOT);

        FIRST.store(false, Ordering::Release);
    }

    trap::trap_user_ret(private);
}

/// Automatically releases `lock` and sleeps on `chan``.
///
/// Reacquires lock when awakened.
pub fn sleep<T>(chan: *const c_void, guard: SpinLockGuard<'_, T>) -> SpinLockGuard<'_, T> {
    let p = ProcShared::current();
    // Must acquire `p.lock` in order to change
    // `p.state` and then call `sched()`.
    // Once we hold `p.lock()`, we can be
    // guaranteed that we won't miss any wakeup
    // (wakeup locks `p.lock`),
    // so it's okay to release `lock' here.`
    let mut shared = p.lock();
    let lock = guard.into_lock();

    // Go to sleep.
    shared.state = ProcState::Sleeping { chan };

    scheduler::sched(&mut shared);

    // Reacquire original lock.
    drop(shared);
    lock.lock()
}

/// Wakes up all processes sleeping on `chan`.
///
/// Must be called without any processes locked.
pub fn wakeup(chan: *const c_void) {
    for p in &PROC {
        let mut shared = p.shared.lock();
        if let ProcState::Sleeping { chan: ch } = shared.state {
            if ch == chan {
                shared.state = ProcState::Runnable;
            }
        }
        drop(shared);
    }
}

/// Kills the process with the given PID.
///
/// The victim won't exit until it tries to return
/// to user spaec (see `usertrap()`).
pub fn kill(pid: ProcId) -> Result<(), Error> {
    for p in &PROC {
        let mut shared = p.shared.lock();
        if shared.pid == pid {
            shared.killed = true;
            if let ProcState::Sleeping { .. } = shared.state {
                // Wake process from sleep().
                shared.state = ProcState::Runnable;
            }
            drop(shared);
            return Ok(());
        }
        drop(shared);
    }
    Err(Error::Unknown)
}

/// Copies to either a user address, or kernel address,
/// depending on `user_dst`.
pub fn either_copy_out_bytes(
    p_private: &ProcPrivateData,
    user_dst: bool,
    dst: usize,
    src: &[u8],
) -> Result<(), Error> {
    if user_dst {
        return vm::copy_out_bytes(p_private.pagetable().unwrap(), VirtAddr::new(dst), src);
    }

    unsafe {
        let dst = ptr::with_exposed_provenance_mut::<u8>(dst);
        let dst = slice::from_raw_parts_mut(dst, src.len());
        dst.copy_from_slice(src);
        Ok(())
    }
}

/// Copies from either a user address, or kernel address,
/// depending on `user_src`.
pub fn either_copy_in_bytes(
    p_private: &ProcPrivateData,
    dst: &mut [u8],
    user_src: bool,
    src: usize,
) -> Result<(), Error> {
    if user_src {
        return vm::copy_in_bytes(p_private.pagetable().unwrap(), dst, VirtAddr::new(src));
    }
    unsafe {
        let src = ptr::with_exposed_provenance::<u8>(src);
        let src = slice::from_raw_parts(src, dst.len());
        dst.copy_from_slice(src);
        Ok(())
    }
}

/// Prints a process listing to console.
///
/// For debugging.
/// Runs when user type ^P on console
pub fn dump() {
    println!();
    for p in &PROC {
        let shared = p.shared.lock();
        let pid = shared.pid;
        let state = shared.state;
        let name = shared.name;
        drop(shared);
        if state == ProcState::Unused {
            continue;
        }

        let state = match state {
            ProcState::Unused => "unused",
            ProcState::Used => "used",
            ProcState::Sleeping { .. } => "sleep",
            ProcState::Runnable => "runble",
            ProcState::Running => "run",
            ProcState::Zombie { .. } => "zombie",
        };

        println!("{pid:5} {state:<10} {name}");
    }
}
