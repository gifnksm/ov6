use core::{
    cell::UnsafeCell,
    ffi::c_int,
    mem,
    ptr::{self, NonNull},
    sync::atomic::{AtomicI32, Ordering},
};

pub const CONSOLE: usize = 1;

mod ffi {

    use core::ptr;

    use super::*;

    #[unsafe(no_mangle)]
    extern "C" fn filealloc() -> *mut File {
        super::alloc().map_or(ptr::null_mut(), |f| ptr::from_ref(f).cast_mut())
    }

    #[unsafe(no_mangle)]
    extern "C" fn filedup(f: *mut File) -> *mut File {
        ptr::from_ref(super::dup(unsafe { f.as_ref() }.unwrap())).cast_mut()
    }

    #[unsafe(no_mangle)]
    extern "C" fn fileclose(f: *mut File) {
        super::close(unsafe { f.as_mut() }.unwrap())
    }

    #[unsafe(no_mangle)]
    extern "C" fn filestat(f: *mut File, addr: u64) -> c_int {
        let p = Proc::myproc().unwrap();
        let f = unsafe { f.as_mut() }.unwrap();
        let addr = VirtAddr::new(addr as usize);
        match super::stat(p, f, addr) {
            Ok(()) => 0,
            Err(()) => -1,
        }
    }

    #[unsafe(no_mangle)]
    extern "C" fn fileread(f: *mut File, addr: u64, n: c_int) -> c_int {
        let f = unsafe { f.as_mut() }.unwrap();
        let addr = VirtAddr::new(addr as usize);
        match super::read(f, addr, n as usize) {
            Ok(sz) => sz as c_int,
            Err(()) => -1,
        }
    }

    #[unsafe(no_mangle)]
    extern "C" fn filewrite(f: *mut File, addr: u64, n: c_int) -> c_int {
        let f = unsafe { f.as_mut() }.unwrap();
        let addr = VirtAddr::new(addr as usize);
        match super::write(f, addr, n as usize) {
            Ok(sz) => sz as c_int,
            Err(()) => -1,
        }
    }
}

use crate::{
    bio::BLOCK_SIZE,
    fs::{self, NDIRECT},
    log,
    param::{MAX_OP_BLOCKS, NDEV, NFILE},
    pipe::{self, Pipe},
    proc::Proc,
    sleeplock::SleepLock,
    spinlock::SpinLock,
    stat::Stat,
    vm::{self, VirtAddr},
};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(i32)]
pub enum FileType {
    None = 0,
    Pipe,
    Inode,
    Device,
}

#[repr(C)]
pub struct File {
    pub ty: FileType,
    /// Reference count.
    refcnt: AtomicI32,
    pub readable: u8,
    pub writable: u8,
    // FileType::Pipe
    pub pipe: Option<NonNull<Pipe>>,
    // FileType::Inode & FileType::Device
    ip: Option<NonNull<Inode>>,
    // FileTYpe::Inode
    off: UnsafeCell<u32>,
    // FileType::Device
    major: i16,
}

impl File {
    const fn zero() -> Self {
        File {
            ty: FileType::None,
            refcnt: AtomicI32::new(0),
            readable: 0,
            writable: 0,
            pipe: None,
            ip: None,
            off: UnsafeCell::new(0),
            major: 0,
        }
    }

    pub fn init_read_pipe(&mut self, pipe: NonNull<Pipe>) {
        assert_eq!(self.ty, FileType::None);
        self.ty = FileType::Pipe;
        self.readable = 1;
        self.writable = 0;
        self.pipe = Some(pipe);
    }

    pub fn init_write_pipe(&mut self, pipe: NonNull<Pipe>) {
        assert_eq!(self.ty, FileType::None);
        self.ty = FileType::Pipe;
        self.readable = 0;
        self.writable = 1;
        self.pipe = Some(pipe);
    }
}

/// In-memory copy of an inode.
#[repr(C)]
pub struct Inode {
    /// Device number
    dev: u32,
    /// Inode number
    inum: u32,
    /// Reference count
    refcount: i32,
    /// Protects everything below here.
    lock: SleepLock,
    /// Inode has been read from disk?
    valid: i32,

    // Copy of disk inode
    ty: i16,
    major: i16,
    minor: i16,
    nlink: i16,
    size: u32,
    addrs: [u32; NDIRECT + 1],
}

/// Maps major device number to device functions.
#[repr(C)]
pub struct DevSw {
    pub read: Option<extern "C" fn(c_int, u64, c_int) -> c_int>,
    pub write: Option<extern "C" fn(c_int, u64, c_int) -> c_int>,
}

const _: () = {
    assert!(mem::size_of::<DevSw>() == 16);
};

#[repr(C)]
struct FileTable {
    lock: SpinLock,
    file: [File; NFILE],
}

#[unsafe(export_name = "devsw")]
pub static mut DEVSW: [DevSw; NDEV] = [const {
    DevSw {
        read: None,
        write: None,
    }
}; NDEV];

#[unsafe(export_name = "ftable")]
static mut FTABLE: FileTable = FileTable {
    lock: SpinLock::new(c"ftable"),
    file: [const { File::zero() }; NFILE],
};

/// Allocates a file structure.
pub fn alloc() -> Option<&'static mut File> {
    let ftable = unsafe { (&raw mut FTABLE).as_mut() }.unwrap();

    ftable.lock.acquire();
    let f = ftable.file.iter_mut().find(|f| {
        f.refcnt
            .compare_exchange(0, 1, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    });
    ftable.lock.release();
    f
}

/// Increments ref count for file `f`.
pub fn dup(f: &File) -> &File {
    let ftable = unsafe { (&raw mut FTABLE).as_mut() }.unwrap();

    ftable.lock.acquire();
    let old = f.refcnt.fetch_add(1, Ordering::Relaxed);
    assert!(old >= 1);
    ftable.lock.release();
    f
}

/// Closes file `f`.
///
/// Decrements ref count, closes when reaches 0.
pub fn close(f: &File) {
    let ftable = unsafe { (&raw mut FTABLE).as_mut() }.unwrap();

    ftable.lock.acquire();
    let old = f.refcnt.fetch_sub(1, Ordering::Relaxed);
    assert!(old >= 1);
    if old > 1 {
        ftable.lock.release();
        return;
    }

    // if refcnt is zero, no other reference exists, so we can make mutable reference for `f`.
    let f_ptr = ptr::from_ref(f);
    let f = unsafe { f_ptr.cast_mut().as_mut().unwrap() };

    let mut ff = File::zero();
    mem::swap(&mut ff, f);
    assert_eq!(f.refcnt.load(Ordering::Relaxed), 0);
    assert_eq!(f.ty, FileType::None);
    ftable.lock.release();

    match ff.ty {
        FileType::Pipe => pipe::close(ff.pipe.unwrap(), ff.writable != 0),
        FileType::Inode | FileType::Device => {
            log::begin_op();
            fs::inode_put(ff.ip.unwrap());
            log::end_op();
        }
        _ => {}
    }
}

/// Gets metadata about file `f`.
///
/// `addr` is a user virtual address, pointing to a struct stat.
pub fn stat(p: &Proc, f: &File, addr: VirtAddr) -> Result<(), ()> {
    match f.ty {
        FileType::Inode | FileType::Device => {
            let mut st = Stat::zero();
            fs::inode_lock(f.ip.unwrap());
            fs::stat_inode(f.ip.unwrap(), &mut st);
            fs::inode_unlock(f.ip.unwrap());
            let src: [u8; size_of::<Stat>()] = unsafe { mem::transmute(st) };
            vm::copy_out(p.pagetable().unwrap(), addr, &src)?;
            Ok(())
        }
        _ => Err(()),
    }
}

/// Reads from file `f`.
///
/// `addr` is a user virtual address.
pub fn read(f: &File, addr: VirtAddr, n: usize) -> Result<usize, ()> {
    if f.readable == 0 {
        return Err(());
    }

    match f.ty {
        FileType::None => panic!(),
        FileType::Pipe => pipe::read(unsafe { f.pipe.unwrap().as_ref() }, addr, n),
        FileType::Device => {
            let devsw = unsafe { (&raw const DEVSW).as_ref() }.unwrap();
            let Some(dev) = devsw.get(f.major as usize) else {
                return Err(());
            };
            let Some(read) = dev.read else {
                return Err(());
            };
            let sz = read(1, addr.addr() as u64, n as i32);
            if sz < 0 {
                return Err(());
            }
            Ok(sz as usize)
        }
        FileType::Inode => {
            fs::inode_lock(f.ip.unwrap());
            let res = fs::read_inode(f.ip.unwrap(), true, addr, unsafe { *f.off.get() }, n);
            if let Ok(sz) = res {
                unsafe { *f.off.get() += sz as u32 };
            }
            fs::inode_unlock(f.ip.unwrap());
            res
        }
    }
}

/// Writes to file `f`.
///
/// `addr` is a user virtual address.
pub fn write(f: &File, addr: VirtAddr, n: usize) -> Result<usize, ()> {
    if f.writable == 0 {
        return Err(());
    }

    match f.ty {
        FileType::None => panic!(),
        FileType::Pipe => pipe::write(unsafe { f.pipe.unwrap().as_ref() }, addr, n),
        FileType::Device => {
            let devsw = unsafe { (&raw const DEVSW).as_ref() }.unwrap();
            let Some(dev) = devsw.get(f.major as usize) else {
                return Err(());
            };
            let Some(write) = dev.write else {
                return Err(());
            };
            let sz = write(1, addr.addr() as u64, n as i32);
            if sz < 0 {
                return Err(());
            }
            Ok(sz as usize)
        }
        FileType::Inode => {
            // write a few blocks at a time to avoid exceeding
            // the maximum log transaction size, including
            // i-node, indirect block, allocation blocks,
            // and 2 blocks of slop for non-aligned writes.
            // this really belongs lower down, since write_inode()
            // might be writing a device like the console.
            let max = ((MAX_OP_BLOCKS - 1 - 1 - 2) / 2) * BLOCK_SIZE;
            let mut i = 0;
            while i < n {
                let mut n1 = n - i;
                if n1 > max {
                    n1 = max;
                }

                log::begin_op();
                fs::inode_lock(f.ip.unwrap());
                let res = fs::write_inode(
                    f.ip.unwrap(),
                    true,
                    addr.byte_add(i),
                    unsafe { *f.off.get() },
                    n1,
                );
                if let Ok(sz) = res {
                    unsafe { *f.off.get() += sz as u32 };
                }
                fs::inode_unlock(f.ip.unwrap());
                log::end_op();

                if res != Ok(n1) {
                    // error from write_inode
                    break;
                }
                i += n1;
            }
            if i == n { Ok(n) } else { Err(()) }
        }
    }
}
