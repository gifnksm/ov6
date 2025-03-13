use alloc::boxed::Box;
use core::{
    alloc::AllocError,
    cell::UnsafeCell,
    cmp,
    ffi::c_void,
    mem,
    num::NonZero,
    ops::{Deref, DerefMut},
    panic::Location,
    ptr,
    sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering},
};

use arrayvec::ArrayVec;
use dataview::{Pod, PodMethods as _};
use once_init::OnceInit;
use ov6_syscall::{RegisterValue as _, ReturnType, UserMutRef, syscall as sys};
use ov6_types::{fs::RawFd, os_str::OsStr, path::Path, process::ProcId};

use self::{
    scheduler::Context,
    wait_lock::{Parent, WaitLock},
};
use crate::{
    cpu::Cpu,
    error::KernelError,
    file::File,
    fs::{self, DeviceNo, Inode},
    interrupt::{self, trap},
    memory::{
        PAGE_SIZE, VirtAddr,
        addr::{GenericMutSlice, GenericSlice},
        layout::{self, KSTACK_PAGES},
        page::PageFrameAllocator,
        page_table::PtEntryFlags,
        vm_user::UserPageTable,
    },
    param::{NOFILE, NPROC},
    println,
    sync::{SpinLock, SpinLockCondVar, SpinLockGuard, TryLockError},
    syscall::ReturnValue,
};

mod elf;
pub mod exec;
pub mod scheduler;
mod wait_lock;

static PROC: [Proc; NPROC] = [const { Proc::new() }; NPROC];
static INIT_PROC: OnceInit<&'static Proc> = OnceInit::new();

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod)]
pub struct TrapFrame {
    /// Kernel page table.
    pub kernel_satp: usize, // 0
    /// Top of process's kernel stack.
    pub kernel_sp: usize, // 8
    /// usertrap.
    pub kernel_trap: usize, // 16
    /// Saved user program counter.
    pub epc: usize, // 24
    /// saved kernel tp
    pub kernel_hartid: usize, // 32
    pub ra: usize,  // 40
    pub sp: usize,  // 48
    pub gp: usize,  // 56
    pub tp: usize,  // 64
    pub t0: usize,  // 72
    pub t1: usize,  // 80
    pub t2: usize,  // 88
    pub s0: usize,  // 96
    pub s1: usize,  // 104
    pub a0: usize,  // 112
    pub a1: usize,  // 120
    pub a2: usize,  // 128
    pub a3: usize,  // 136
    pub a4: usize,  // 144
    pub a5: usize,  // 152
    pub a6: usize,  // 160
    pub a7: usize,  // 168
    pub s2: usize,  // 176
    pub s3: usize,  // 184
    pub s4: usize,  // 192
    pub s5: usize,  // 200
    pub s6: usize,  // 208
    pub s7: usize,  // 216
    pub s8: usize,  // 224
    pub s9: usize,  // 232
    pub s10: usize, // 240
    pub s11: usize, // 248
    pub t3: usize,  // 256
    pub t4: usize,  // 264
    pub t5: usize,  // 272
    pub t6: usize,  // 280
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
    pid: Option<ProcId>,
    /// Process name (for debugging)
    name: ArrayVec<u8, 16>,
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
        self.pid.unwrap()
    }

    pub fn name(&self) -> &OsStr {
        OsStr::from_bytes(&self.name)
    }

    pub fn set_name(&mut self, name: &OsStr) {
        self.name.clear();
        let len = usize::min(self.name.capacity(), name.len());
        self.name
            .try_extend_from_slice(&name.as_bytes()[..len])
            .unwrap();
    }

    pub fn kill(&mut self) {
        self.killed = true;
    }

    pub fn killed(&self) -> bool {
        self.killed
    }
}

pub struct ProcShared(SpinLock<ProcSharedData>);

impl ProcShared {
    const fn new() -> Self {
        Self(SpinLock::new(ProcSharedData {
            pid: None,
            name: ArrayVec::new_const(),
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

    pub fn try_lock(&self) -> Result<SpinLockGuard<ProcSharedData>, TryLockError> {
        self.0.try_lock()
    }

    unsafe fn remember_locked(&self) -> SpinLockGuard<ProcSharedData> {
        unsafe { self.0.remember_locked() }
    }
}

pub struct ProcPrivateData {
    pid: Option<ProcId>,
    /// Virtual address of kernel stack.
    kstack: VirtAddr,
    /// User page table,
    pagetable: Option<UserPageTable>,
    /// Data page for trampoline.S
    trapframe: Option<Box<TrapFrame, PageFrameAllocator>>,
    /// Open files
    ofile: [Option<File>; NOFILE],
    /// Current directory
    cwd: Option<Inode>,
}

impl ProcPrivateData {
    pub fn kstack(&self) -> VirtAddr {
        self.kstack
    }

    pub fn size(&self) -> usize {
        self.pagetable.as_ref().unwrap().size()
    }

    pub fn pagetable(&self) -> Option<&UserPageTable> {
        self.pagetable.as_ref()
    }

    pub fn pagetable_mut(&mut self) -> Option<&mut UserPageTable> {
        self.pagetable.as_mut()
    }

    pub fn update_pagetable(&mut self, pt: UserPageTable) {
        self.pagetable.replace(pt);
    }

    pub fn trapframe(&self) -> Option<&TrapFrame> {
        self.trapframe.as_deref()
    }

    pub fn trapframe_mut(&mut self) -> Option<&mut TrapFrame> {
        self.trapframe.as_deref_mut()
    }

    pub fn ofile(&self, fd: RawFd) -> Result<&File, KernelError> {
        self.ofile
            .get(fd.get())
            .and_then(|x| x.as_ref())
            .ok_or_else(|| KernelError::FileDescriptorNotFound(fd, self.pid.unwrap()))
    }

    pub fn add_ofile(&mut self, file: File) -> Result<RawFd, KernelError> {
        let (fd, slot) = self
            .ofile
            .iter_mut()
            .enumerate()
            .find(|(_, slot)| slot.is_none())
            .ok_or(KernelError::NoFreeFileDescriptorTableEntry)?;
        assert!(slot.replace(file).is_none());
        Ok(RawFd::new(fd))
    }

    pub fn unset_ofile(&mut self, fd: RawFd) -> Option<File> {
        self.ofile.get_mut(fd.get())?.take()
    }

    pub fn cwd(&self) -> Option<&Inode> {
        self.cwd.as_ref()
    }

    pub fn update_cwd(&mut self, cwd: Inode) -> Inode {
        self.cwd.replace(cwd).unwrap()
    }
}

pub struct ProcPrivateDataGuard<'p> {
    private_taken: &'p AtomicBool,
    private: &'p mut ProcPrivateData,
}

impl Drop for ProcPrivateDataGuard<'_> {
    fn drop(&mut self) {
        self.private_taken.store(false, Ordering::Release);
    }
}

impl Deref for ProcPrivateDataGuard<'_> {
    type Target = ProcPrivateData;

    fn deref(&self) -> &Self::Target {
        self.private
    }
}

impl DerefMut for ProcPrivateDataGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.private
    }
}

/// Per-process state.
pub struct Proc {
    /// Process sharead data
    shared: ProcShared,
    /// Parent process
    parent: Parent,
    child_ended: SpinLockCondVar,
    /// `true` if `private` is referenced
    private_borrowed: AtomicBool,
    taken_location: AtomicPtr<Location<'static>>,
    /// Process private data.
    private: UnsafeCell<Option<ProcPrivateData>>,
}

unsafe impl Sync for Proc {}

impl Proc {
    const fn new() -> Self {
        Self {
            shared: ProcShared::new(),
            parent: Parent::new(),
            child_ended: SpinLockCondVar::new(),
            private_borrowed: AtomicBool::new(false),
            taken_location: AtomicPtr::new(ptr::from_ref(Location::caller()).cast_mut()),
            private: UnsafeCell::new(None),
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

    #[track_caller]
    #[expect(clippy::mut_from_ref)]
    fn borrow_private_raw(&self) -> &mut Option<ProcPrivateData> {
        if self.private_borrowed.swap(true, Ordering::Acquire) {
            let taker = unsafe { self.taken_location.load(Ordering::Relaxed).as_ref() }.unwrap();
            panic!("ProcPrivateData is already taken at {taker}");
        }
        self.taken_location.store(
            ptr::from_ref(Location::caller()).cast_mut(),
            Ordering::Relaxed,
        );

        unsafe { self.private.get().as_mut().unwrap() }
    }

    #[track_caller]
    fn init_private(&self, data: ProcPrivateData) -> ProcPrivateDataGuard {
        let private = self.borrow_private_raw();
        assert!(private.is_none());
        *private = Some(data);

        ProcPrivateDataGuard {
            private_taken: &self.private_borrowed,
            private: private.as_mut().unwrap(),
        }
    }

    fn drop_private(&self, guard: ProcPrivateDataGuard) {
        // to avoid two multiple mutable reference to `ProcPrivateData` exists
        // concurrently
        let addr = ptr::from_mut(guard.private).addr();
        mem::forget(guard);

        let private_opt = unsafe { self.private.get().as_mut().unwrap() };
        assert_eq!(
            private_opt
                .as_mut()
                .map_or(ptr::null_mut(), ptr::from_mut)
                .addr(),
            addr
        );
        let private = unsafe { self.private.get().as_mut().unwrap() }
            .take()
            .unwrap();
        assert!(private.ofile.iter().all(Option::is_none));
        assert!(private.cwd.is_none());

        self.private_borrowed.store(false, Ordering::Release);
    }

    #[track_caller]
    pub fn borrow_private(&self) -> Option<ProcPrivateDataGuard> {
        let Some(private) = self.borrow_private_raw() else {
            // process already exited
            self.private_borrowed.store(false, Ordering::Release);
            return None;
        };

        Some(ProcPrivateDataGuard {
            private_taken: &self.private_borrowed,
            private,
        })
    }

    fn is_child_of(&self, parent: &Self, wait_lock: &mut SpinLockGuard<WaitLock>) -> bool {
        self.parent
            .get(wait_lock)
            .is_some_and(|pp| ptr::eq(parent, pp))
    }

    fn set_parent(&self, parent: &'static Self, wait_lock: &mut SpinLockGuard<WaitLock>) {
        self.parent.set(parent, wait_lock);
    }

    fn allocate_pid() -> ProcId {
        static NEXT_PID: AtomicU32 = AtomicU32::new(1);
        let pid = NEXT_PID.fetch_add(1, Ordering::Relaxed);
        ProcId::new(NonZero::new(pid).unwrap())
    }

    /// Returns UNUSED proc in the process table.
    ///
    /// If there is no UNUSED proc, returns None.
    /// This function also locks the proc.
    fn lock_unused_proc()
    -> Result<(usize, &'static Self, SpinLockGuard<'static, ProcSharedData>), KernelError> {
        for (i, p) in PROC.iter().enumerate() {
            let shared = p.shared.lock();
            if shared.state != ProcState::Unused {
                drop(shared);
                continue;
            }
            return Ok((i, p, shared));
        }
        Err(KernelError::NoFreeProc)
    }

    /// Returns a new process.
    ///
    /// Locks in the process table for an UNUSED proc.
    /// If found, initialize state required to run in the kenrnel,
    /// and return with the lock held.
    /// If there are no free procs, return None.
    fn allocate() -> Result<
        (
            &'static Self,
            SpinLockGuard<'static, ProcSharedData>,
            ProcPrivateDataGuard<'static>,
        ),
        KernelError,
    > {
        let (i, p, mut shared) = Self::lock_unused_proc()?;

        let pid = Self::allocate_pid();
        shared.pid = Some(pid);
        shared.state = ProcState::Used;

        let res: Result<ProcPrivateData, KernelError> = (|| {
            let trapframe = Box::try_new_in(TrapFrame::zeroed(), PageFrameAllocator)
                .map_err(|AllocError| KernelError::NoFreePage)?;

            let private = ProcPrivateData {
                pid: Some(pid),
                kstack: layout::kstack(i),
                pagetable: Some(UserPageTable::new(&trapframe)?),
                trapframe: Some(trapframe),
                ofile: [const { None }; NOFILE],
                cwd: None,
            };

            // Set up new context to start executing at forkret,
            // which returns to user space.
            shared.context.clear();
            shared.context.ra = forkret as usize;
            shared.context.sp = private.kstack.byte_add(KSTACK_PAGES * PAGE_SIZE).addr();
            Ok(private)
        })();

        let private = match res {
            Ok(private) => private,
            Err(e) => {
                p.free(&mut shared);
                return Err(e);
            }
        };

        let private = p.init_private(private);

        Ok((p, shared, private))
    }

    /// Frees a proc structure and the data hangind from it,
    /// including user pages.
    ///
    /// p.lock must be held.
    fn free(&self, shared: &mut SpinLockGuard<ProcSharedData>) {
        unsafe {
            self.parent.reset();
        }
        shared.pid = None;
        shared.name.clear();
        shared.killed = false;

        shared.state = ProcState::Unused;
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
    let (p, mut shared, mut private) = Proc::allocate().unwrap();
    assert_eq!(shared.pid.unwrap().get().get(), 1);
    INIT_PROC.init(p);

    // allocate one user page and copy initcode's instructions
    // and data into it.
    private
        .pagetable_mut()
        .unwrap()
        .map_first(INIT_CODE)
        .unwrap();

    // prepare for the very first `return` from kernel to user.
    let trapframe = private.trapframe_mut().unwrap();
    trapframe.epc = 0; // user program counter
    trapframe.sp = PAGE_SIZE; // user stack pointer

    let tx = fs::begin_readonly_tx();
    private.cwd = Some(Inode::from_tx(
        &fs::path::resolve(&tx, &mut private, Path::new("/")).unwrap(),
    ));
    tx.end();
    shared.set_name(OsStr::new("initcode"));
    shared.state = ProcState::Runnable;

    drop(shared);
}

/// Grows user memory by `n` Bytes.
pub fn grow_proc(private: &mut ProcPrivateData, increment: isize) -> Result<(), KernelError> {
    let pagetable = private.pagetable_mut().unwrap();
    let old_sz = pagetable.size();
    let new_sz = old_sz.saturating_add_signed(increment);
    match new_sz.cmp(&old_sz) {
        cmp::Ordering::Less => pagetable.shrink_to(new_sz),
        cmp::Ordering::Equal => {}
        cmp::Ordering::Greater => pagetable.grow_to(new_sz, PtEntryFlags::W)?,
    }
    Ok(())
}

/// Creates a new process, copying the parent.
///
/// Sets up child kernel stack to return as if from `fork()` system call.
pub fn fork(p: &'static Proc, p_private: &ProcPrivateData) -> Result<ProcId, KernelError> {
    let parent_name = p.shared().lock().name.clone();

    // Allocate process.
    let (np, mut np_shared, mut np_private) = Proc::allocate()?;

    // Copy use memory from parent to child.
    if let Err(e) = p_private
        .pagetable()
        .unwrap()
        .try_clone(np_private.pagetable_mut().unwrap())
    {
        np.free(&mut np_shared);
        drop(np_shared);
        return Err(e);
    }

    // Copy saved user registers.
    *np_private.trapframe_mut().unwrap() = *p_private.trapframe().unwrap();

    // Cause fork to return 0 in the child.
    let child_ret: ReturnType<sys::Fork> = Ok(None);
    ReturnValue::from(child_ret.encode()).store(np_private.trapframe_mut().unwrap());

    // increment refereence counts on open file descriptors.
    for (of, nof) in p_private.ofile.iter().zip(&mut np_private.ofile) {
        if let Some(of) = of {
            *nof = Some(of.dup());
        }
    }
    np_private.cwd.clone_from(&p_private.cwd);
    np_shared.name = parent_name;

    let pid = np_shared.pid.unwrap();
    drop(np_shared);

    let mut wait_lock = wait_lock::lock();
    np.parent.set(p, &mut wait_lock);
    drop(wait_lock);

    // After setting the state to Runnable, the scheduler can pick up `np` and the
    // process context may start. The started process context (e.g., forkret)
    // will refer to `ProcPrivateData`, so we must drop `np_private` here.
    drop(np_private);
    np.shared.lock().state = ProcState::Runnable;

    Ok(pid)
}

/// Pass p's abandoned children to init.
///
/// Caller must hold `WAIT_LOCK`
fn reparent(old_parent: &Proc, new_parent: &'static Proc, wait_lock: &mut SpinLockGuard<WaitLock>) {
    for pp in &PROC {
        if pp.is_child_of(old_parent, wait_lock) {
            pp.set_parent(new_parent, wait_lock);
            new_parent.child_ended.notify();
        }
    }
}

/// Exits the current process.
///
/// Does not return.
/// An exited process remains in the zombie state
/// until its parent calls `wait()`.
pub fn exit(p: &Proc, mut p_private: ProcPrivateDataGuard, status: i32) -> ! {
    let init_proc = *INIT_PROC.get();

    // Ensure all destruction is done before `sched().`
    let mut shared = {
        assert!(!ptr::eq(p, init_proc), "init exiting");

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
        reparent(p, init_proc, &mut wait_lock);

        // Parent might be sleeping in wait().
        if let Some(parent) = p.parent.get(&mut wait_lock) {
            parent.child_ended.notify();
        }

        let mut shared = p.shared.lock();
        shared.state = ProcState::Zombie {
            exit_status: status,
        };

        p.drop_private(p_private);

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
pub fn wait(
    p: &Proc,
    p_private: &mut ProcPrivateData,
    user_status: UserMutRef<i32>,
) -> Result<ProcId, KernelError> {
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
                let pid = pp_shared.pid.unwrap();
                if user_status.addr() != 0 {
                    p_private
                        .pagetable_mut()
                        .unwrap()
                        .copy_out(user_status, &exit_status)?;
                }
                pp.free(&mut pp_shared);
                return Ok(pid);
            }
        }

        // No point waiting if we don't have any children.
        if !have_kids || p.shared.lock().killed() {
            drop(wait_lock);
            return Err(KernelError::NoChildProcess);
        }

        // Wait for a child to exit.
        wait_lock = p.child_ended.wait(wait_lock);
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
    let _ = unsafe { p.shared.remember_locked() }; // unlock here
    let Some(private) = p.borrow_private() else {
        // process is already exited (in zombie state)
        yield_(p);
        unreachable!();
    };

    if FIRST.load(Ordering::Acquire) {
        // File system initialization must be run in the context of a
        // regular process (e.g., because it calls sleep), and thus cannot
        // be run from main().
        fs::init_in_proc(DeviceNo::ROOT);

        FIRST.store(false, Ordering::Release);
    }

    trap::trap_user_ret(private);
}

/// Automatically releases `lock` and sleeps on `chan`.
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
pub fn kill(pid: ProcId) -> Result<(), KernelError> {
    for p in &PROC {
        let mut shared = p.shared.lock();
        if shared.pid == Some(pid) {
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
    Err(KernelError::ProcessNotFound(pid))
}

/// Copies to either a user address, or kernel address,
/// depending on `user_dst`.
pub fn either_copy_out_bytes(
    p_private: &mut ProcPrivateData,
    dst: GenericMutSlice<u8>,
    src: &[u8],
) -> Result<(), KernelError> {
    assert_eq!(dst.len(), src.len());
    match dst {
        GenericMutSlice::User(dst) => p_private
            .pagetable_mut()
            .unwrap()
            .copy_out_bytes(dst, src)?,
        GenericMutSlice::Kernel(dst) => dst.copy_from_slice(src),
    }
    Ok(())
}

/// Copies from either a user address, or kernel address,
/// depending on `user_src`.
pub fn either_copy_in_bytes(
    p_private: &ProcPrivateData,
    dst: &mut [u8],
    src: GenericSlice<u8>,
) -> Result<(), KernelError> {
    assert_eq!(dst.len(), src.len());
    match src {
        GenericSlice::User(src) => p_private.pagetable().unwrap().copy_in_bytes(dst, src)?,
        GenericSlice::Kernel(src) => dst.copy_from_slice(src),
    }
    Ok(())
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
        let name = shared.name.clone();
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

        let pid = pid.unwrap();
        let name = OsStr::from_bytes(&name).display();
        println!("{pid:5} {state:<10} {name}");
    }
}
