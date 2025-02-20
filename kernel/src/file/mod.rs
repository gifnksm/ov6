use core::{
    cell::UnsafeCell,
    ffi::c_int,
    mem,
    ptr::{self, NonNull},
    sync::atomic::{AtomicI32, Ordering},
};

use crate::{
    fs::{self, Inode, block_io::BLOCK_SIZE},
    memory::vm::{self, VirtAddr},
    param::{MAX_OP_BLOCKS, NDEV, NFILE},
    proc::Proc,
    sync::RawSpinLock,
};

use self::pipe::Pipe;

pub const CONSOLE: usize = 1;

pub mod pipe;

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

    pub fn init_device(&mut self, major: i16, ip: NonNull<Inode>, readable: bool, writable: bool) {
        assert_eq!(self.ty, FileType::None);
        self.ty = FileType::Device;
        self.readable = readable as u8;
        self.writable = writable as u8;
        self.major = major;
        self.ip = Some(ip);
    }

    pub fn init_inode(&mut self, ip: NonNull<Inode>, readable: bool, writable: bool) {
        assert_eq!(self.ty, FileType::None);
        self.ty = FileType::Inode;
        self.readable = readable as u8;
        self.writable = writable as u8;
        *self.off.get_mut() = 0;
        self.ip = Some(ip);
    }
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
    lock: RawSpinLock,
    file: [File; NFILE],
}

pub static mut DEVSW: [DevSw; NDEV] = [const {
    DevSw {
        read: None,
        write: None,
    }
}; NDEV];

static mut FTABLE: FileTable = FileTable {
    lock: RawSpinLock::new(),
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
            let tx = fs::begin_tx();
            fs::inode_put(&tx, ff.ip.unwrap());
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
            let tx = fs::begin_readonly_tx();
            let st = fs::inode_with_lock(&tx, f.ip.unwrap(), fs::stat_inode);
            vm::copy_out(p.pagetable().unwrap(), addr, &st)?;
            Ok(())
        }
        _ => Err(()),
    }
}

/// Reads from file `f`.
///
/// `addr` is a user virtual address.
pub fn read(p: &Proc, f: &File, addr: VirtAddr, n: usize) -> Result<usize, ()> {
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
            let tx = fs::begin_readonly_tx();
            fs::inode_with_lock(&tx, f.ip.unwrap(), |ip| {
                let res =
                    fs::read_inode(&tx, p, ip, true, addr, unsafe { *f.off.get() } as usize, n);
                if let Ok(sz) = res {
                    unsafe { *f.off.get() += sz as u32 };
                }
                res
            })
        }
    }
}

/// Writes to file `f`.
///
/// `addr` is a user virtual address.
pub fn write(p: &Proc, f: &File, addr: VirtAddr, n: usize) -> Result<usize, ()> {
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

                let tx = fs::begin_tx();
                let res = fs::inode_with_lock(&tx, f.ip.unwrap(), |ip| {
                    let res = fs::write_inode(
                        &tx,
                        p,
                        ip,
                        true,
                        addr.byte_add(i),
                        unsafe { *f.off.get() } as usize,
                        n1,
                    );
                    if let Ok(sz) = res {
                        unsafe { *f.off.get() += sz as u32 };
                    }
                    res
                });
                tx.end();

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
