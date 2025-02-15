use core::{ptr, sync::atomic::Ordering};

use crate::{
    bio::{BLOCK_SIZE, Buf},
    kalloc,
    memlayout::VIRTIO0,
    proc,
    spinlock::SpinLock,
    virtio::{
        MmioRegister, NUM, VIRTIO_BLK_F_CONFIG_WCE, VIRTIO_BLK_F_MQ, VIRTIO_BLK_F_RO,
        VIRTIO_BLK_F_SCSI, VIRTIO_BLK_T_IN, VIRTIO_BLK_T_OUT, VIRTIO_CONFIG_S_ACKNOWLEDGE,
        VIRTIO_CONFIG_S_DRIVER, VIRTIO_CONFIG_S_DRIVER_OK, VIRTIO_CONFIG_S_FEATURES_OK,
        VIRTIO_F_ANY_LAYOUT, VIRTIO_RING_F_EVENT_IDX, VIRTIO_RING_F_INDIRECT_DESC,
        VRING_DESC_F_NEXT, VRING_DESC_F_WRITE, VirtioBlkReq, VirtqAvail, VirtqDesc, VirtqUsed,
    },
    vm::PAGE_SIZE,
};

#[repr(C)]
struct Disk {
    /// A set (not a ring) of DMA descriptors, with which the
    /// driver tells the device where to read and write individual
    /// disk operations.
    ///
    /// There are NUM descriptors.
    /// Most commands consist of a "chain" (a linked list) of a couple of
    /// these descriptors.
    desc: *mut VirtqDesc,

    /// A ring in which the driver writes descriptor numbers
    /// that the driver would like the device to process.
    ///
    /// It only includes the head descriptor of each chain.
    /// The ring has NUM elements.
    avail: *mut VirtqAvail,

    /// A ring in which the device writes descriptor numbers that
    /// the device has finished processing (just the head of each chain).
    ///
    /// There are NUM used ring entries.
    used: *mut VirtqUsed,

    // our own book-keeping.
    free: [u8; NUM], // is a descriptor free?
    used_idx: u16,   // we've looked this far in used[2..NUM].

    /// Track info about in-flight operations,
    /// for use when completion interrupt arrives.
    ///
    /// Indexed by first descriptor index of chain.
    info: [Info; NUM],

    /// Disk command headers.
    ///
    /// One-for-one with descriptors, for convenience.
    ops: [VirtioBlkReq; NUM],

    vdisk_lock: SpinLock,
}

#[repr(C)]
struct Info {
    b: *mut Buf,
    status: u8,
}

fn reg_read(r: MmioRegister) -> u32 {
    unsafe { ptr::with_exposed_provenance::<u32>(VIRTIO0 + r as usize).read_volatile() }
}

fn reg_write(r: MmioRegister, value: u32) {
    unsafe { ptr::with_exposed_provenance_mut::<u32>(VIRTIO0 + r as usize).write_volatile(value) }
}

fn lock() -> &'static mut Disk {
    static mut DISK: Disk = Disk {
        desc: ptr::null_mut(),
        avail: ptr::null_mut(),
        used: ptr::null_mut(),
        free: [0; NUM],
        used_idx: 0,
        info: [const {
            Info {
                b: ptr::null_mut(),
                status: 0,
            }
        }; NUM],
        ops: [const {
            VirtioBlkReq {
                ty: 0,
                reserved: 0,
                sector: 0,
            }
        }; NUM],
        vdisk_lock: SpinLock::new(c"virtio_disk"),
    };

    unsafe {
        let disk = &raw mut DISK;
        (*disk).vdisk_lock.acquire();
        disk.as_mut().unwrap()
    }
}

pub fn init() {
    assert_eq!(reg_read(MmioRegister::MagicValue), 0x7472_6976);
    assert_eq!(reg_read(MmioRegister::Version), 2);
    assert_eq!(reg_read(MmioRegister::DeviceId), 2);
    assert_eq!(reg_read(MmioRegister::VendorId), 0x554d_4551);

    let disk = lock();

    let mut status = 0;

    // reset device
    reg_write(MmioRegister::Status, status);

    // set ACKNOWLEDGE status bit
    status |= VIRTIO_CONFIG_S_ACKNOWLEDGE;
    reg_write(MmioRegister::Status, status);

    // set DRIVER status bit
    status |= VIRTIO_CONFIG_S_DRIVER;
    reg_write(MmioRegister::Status, status);

    // negotiate features
    let mut features = reg_read(MmioRegister::DeviceFeatures);
    features &= !(1 << VIRTIO_BLK_F_RO);
    features &= !(1 << VIRTIO_BLK_F_SCSI);
    features &= !(1 << VIRTIO_BLK_F_CONFIG_WCE);
    features &= !(1 << VIRTIO_BLK_F_MQ);
    features &= !(1 << VIRTIO_F_ANY_LAYOUT);
    features &= !(1 << VIRTIO_RING_F_EVENT_IDX);
    features &= !(1 << VIRTIO_RING_F_INDIRECT_DESC);
    reg_write(MmioRegister::DriverFeatures, features);

    // tell device that feature negotiation is complete.
    status |= VIRTIO_CONFIG_S_FEATURES_OK;
    reg_write(MmioRegister::Status, status);

    // re-read status to ensure FEATURES_OK is set.
    status = reg_read(MmioRegister::Status);
    assert!((status & VIRTIO_CONFIG_S_FEATURES_OK) != 0);

    // initialize queue 0.
    reg_write(MmioRegister::QueueSel, 0);

    // ensure queue 0 is not in use.
    assert_eq!(reg_read(MmioRegister::QueueReady), 0);

    // check maximum queue size.
    let max = reg_read(MmioRegister::QueueNumMax);
    assert!(max != 0);
    assert!(max as usize >= NUM);

    // allocate and zero queue memory.
    unsafe {
        disk.desc = kalloc::alloc_page().unwrap().as_ptr().cast();
        disk.avail = kalloc::alloc_page().unwrap().as_ptr().cast();
        disk.used = kalloc::alloc_page().unwrap().as_ptr().cast();

        disk.desc.cast::<u8>().write_bytes(0, PAGE_SIZE);
        disk.avail.cast::<u8>().write_bytes(0, PAGE_SIZE);
        disk.used.cast::<u8>().write_bytes(0, PAGE_SIZE);
    }

    // set queue size.
    reg_write(MmioRegister::QueueNum, NUM as u32);

    // write physical addresses.
    fn low(p: usize) -> u32 {
        (p & 0xffff_ffff) as u32
    }
    fn high(p: usize) -> u32 {
        ((p >> 32) & 0xffff_ffff) as u32
    }

    reg_write(MmioRegister::QueueDescLow, low(disk.desc.addr()));
    reg_write(MmioRegister::QueueDescHigh, high(disk.desc.addr()));
    reg_write(MmioRegister::DriverDescLow, low(disk.avail.addr()));
    reg_write(MmioRegister::DriverDescHigh, high(disk.avail.addr()));
    reg_write(MmioRegister::DeviceDescLow, low(disk.used.addr()));
    reg_write(MmioRegister::DeviceDescHigh, high(disk.used.addr()));

    // queue is ready.
    reg_write(MmioRegister::QueueReady, 1);

    // all NUM descriptors start out unused.
    disk.free.fill(1);

    // tell device we're completely ready.
    status |= VIRTIO_CONFIG_S_DRIVER_OK;
    reg_write(MmioRegister::Status, status);

    disk.vdisk_lock.release();
}

/// Finds a free descriptor, marks it non-free, returns its index.
fn alloc_desc(disk: &mut Disk) -> Option<usize> {
    let idx = disk.free.iter().position(|&b| b != 0)?;
    disk.free[idx] = 0;
    Some(idx)
}

/// Marks a descriptor as free.
fn free_desc(disk: &mut Disk, i: usize) {
    assert!(i < NUM);
    assert_eq!(disk.free[i], 0);
    unsafe {
        let desc = disk.desc.add(i);
        *desc = VirtqDesc {
            addr: 0,
            len: 0,
            flags: 0,
            next: 0,
        };
        disk.free[i] = 1;
    };
    proc::wakeup((&raw const disk.free[0]).cast());
}

/// Frees a chain of descriptors.
fn free_chain(disk: &mut Disk, mut i: usize) {
    loop {
        let desc = unsafe { &*disk.desc.add(i) };
        let flag = desc.flags;
        let next = desc.next;
        free_desc(disk, i);
        if flag & VRING_DESC_F_NEXT == 0 {
            break;
        }
        i = next.into();
    }
}

/// Allocates three descriptors (they need not be contiguous).
///
/// Disk transfers always use three descriptors.
fn alloc3_desc(disk: &mut Disk) -> Option<[usize; 3]> {
    let mut idx = [0; 3];
    for i in 0..3 {
        match alloc_desc(disk) {
            Some(x) => idx[i] = x,
            None => {
                for j in &idx[0..i] {
                    free_desc(disk, *j);
                }
                return None;
            }
        }
    }
    Some(idx)
}

fn read_or_write(b: &mut Buf, write: bool) {
    let sector = (b.block_no.unwrap().value() as usize * (BLOCK_SIZE / 512)) as u64;

    let disk = lock();

    // the spec's Section 5.2 says that legacy block operations use
    // three descriptors: one for type/reserved/sector, one for the
    // data, one for a 1-byte status result.

    // allocate three descriptors.
    let idx = loop {
        if let Some(idx) = alloc3_desc(disk) {
            break idx;
        }
        proc::sleep_raw((&raw const disk.free[0]).cast(), &disk.vdisk_lock);
    };

    // format the three descriptors.

    let buf0 = &mut disk.ops[idx[0]];
    *buf0 = VirtioBlkReq {
        ty: if write {
            VIRTIO_BLK_T_OUT // write the disk
        } else {
            VIRTIO_BLK_T_IN // read the disk
        },
        reserved: 0,
        sector,
    };

    unsafe {
        disk.desc.add(idx[0]).write(VirtqDesc {
            addr: ptr::from_mut(buf0).addr() as u64,
            len: size_of::<VirtioBlkReq>() as u32,
            flags: VRING_DESC_F_NEXT,
            next: idx[1] as u16,
        });

        disk.desc.add(idx[1]).write(VirtqDesc {
            addr: (&raw mut b.data).addr() as u64,
            len: BLOCK_SIZE as u32,
            flags: if write {
                0 // device reads b.date
            } else {
                VRING_DESC_F_WRITE // device writes b.data
            } | VRING_DESC_F_NEXT,
            next: idx[2] as u16,
        });

        disk.info[idx[0]].status = 0xff; // device writes 0 on success
        disk.desc.add(idx[2]).write(VirtqDesc {
            addr: (&raw mut disk.info[idx[0]].status).addr() as u64,
            len: 1,
            flags: VRING_DESC_F_WRITE,
            next: 0,
        });
    }

    // record struct buf for `handle_interrupt()`.
    b.disk = 1;
    disk.info[idx[0]].b = b;

    // tell the device the first index in our chain of descriptors.
    unsafe {
        (*disk.avail).ring[(*disk.avail).idx.load(Ordering::Relaxed) as usize % NUM] =
            idx[0] as u16;
    }

    // tell the device another avail ring entry is available.
    unsafe {
        (*disk.avail).idx.fetch_add(1, Ordering::AcqRel);
    }

    reg_write(MmioRegister::QueueNotify, 0); // value is queue number

    // Wait for `handle_interrupts()` to say request has finished.
    while b.disk != 0 {
        proc::sleep_raw(ptr::from_mut(b).cast(), &disk.vdisk_lock);
    }

    disk.info[idx[0]].b = ptr::null_mut();
    free_chain(disk, idx[0]);

    disk.vdisk_lock.release();
}

pub fn read(b: &mut Buf) {
    read_or_write(b, false);
}

pub fn write(b: &mut Buf) {
    read_or_write(b, true)
}

pub fn handle_interrupt() {
    let disk = lock();

    // the device won't raise another interrupt until we tell it
    // we've seen this interrupt, which the following line does.
    // this may race with the device writing new entries to
    // the "used" ring, in which case we may process the new
    // completion entries in this interrupt, and have nothing to do
    // in the next interrupt, which is harmless.
    reg_write(
        MmioRegister::InterruptAck,
        reg_read(MmioRegister::InterruptStatus) & 0x3,
    );

    // the device increments disk.used.idx when it
    // adds an entry to the used ring.

    unsafe {
        while disk.used_idx != (*disk.used).idx.load(Ordering::Acquire) {
            let id = (*disk.used).ring[disk.used_idx as usize % NUM].id;

            assert_eq!(disk.info[id as usize].status, 0);

            let b = disk.info[id as usize].b;
            (*b).disk = 0; // disk is done with buf
            proc::wakeup(b.cast());

            disk.used_idx += 1;
        }
    }

    disk.vdisk_lock.release();
}
