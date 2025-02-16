use core::{
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::{
    bio::BLOCK_SIZE,
    kalloc,
    memlayout::VIRTIO0,
    proc,
    spinlock::SpinLock,
    virtio::{
        BLK_SECTOR_SIZE, ConfigStatus, DeviceFeatures, MmioRegister, VirtioBlkReq,
        VirtioBlkReqType, VirtqAvail, VirtqDesc, VirtqDescFlags, VirtqUsed,
    },
    vm::PAGE_SIZE,
};

// This many virtio descriptors.
// Must be a power of two.
pub const NUM: usize = 8;

struct Disk {
    /// A set (not a ring) of DMA descriptors, with which the
    /// driver tells the device where to read and write individual
    /// disk operations.
    ///
    /// There are NUM descriptors.
    /// Most commands consist of a "chain" (a linked list) of a couple of
    /// these descriptors.
    desc: *mut [VirtqDesc; NUM],

    /// A ring in which the driver writes descriptor numbers
    /// that the driver would like the device to process.
    ///
    /// It only includes the head descriptor of each chain.
    /// The ring has NUM elements.
    avail: *mut VirtqAvail<NUM>,

    /// A ring in which the device writes descriptor numbers that
    /// the device has finished processing (just the head of each chain).
    ///
    /// There are NUM used ring entries.
    used: *mut VirtqUsed<NUM>,

    // our own book-keeping.
    free: [bool; NUM], // is a descriptor free?
    used_idx: u16,     // we've looked this far in used[2..NUM].

    /// Track info about in-flight operations,
    /// for use when completion interrupt arrives.
    ///
    /// Indexed by first descriptor index of chain.
    info: [TrackInfo; NUM],

    /// Disk command headers.
    ///
    /// One-for-one with descriptors, for convenience.
    ops: [VirtioBlkReq; NUM],
}

unsafe impl Send for Disk {}

struct TrackInfo {
    data: *const u8,
    status: u8,
    in_progress: AtomicBool,
}

fn reg_read(r: MmioRegister) -> u32 {
    unsafe { ptr::with_exposed_provenance::<u32>(VIRTIO0 + r as usize).read_volatile() }
}

fn reg_write(r: MmioRegister, value: u32) {
    unsafe { ptr::with_exposed_provenance_mut::<u32>(VIRTIO0 + r as usize).write_volatile(value) }
}

static DISK: SpinLock<Disk> = SpinLock::new(Disk {
    desc: ptr::null_mut(),
    avail: ptr::null_mut(),
    used: ptr::null_mut(),
    free: [false; NUM],
    used_idx: 0,
    info: [const {
        TrackInfo {
            data: ptr::null_mut(),
            status: 0,
            in_progress: AtomicBool::new(false),
        }
    }; NUM],
    ops: [const {
        VirtioBlkReq {
            ty: VirtioBlkReqType::In,
            reserved: 0,
            sector: 0,
        }
    }; NUM],
});

pub fn init() {
    assert_eq!(reg_read(MmioRegister::MagicValue), 0x7472_6976);
    assert_eq!(reg_read(MmioRegister::Version), 2);
    assert_eq!(reg_read(MmioRegister::DeviceId), 2);
    assert_eq!(reg_read(MmioRegister::VendorId), 0x554d_4551);

    let mut disk = DISK.lock();

    let mut status = ConfigStatus::empty();

    // reset device
    reg_write(MmioRegister::Status, status.bits());

    // set ACKNOWLEDGE status bit
    status |= ConfigStatus::ACKNOWLEDGE;
    reg_write(MmioRegister::Status, status.bits());

    // set DRIVER status bit
    status |= ConfigStatus::DRIVER;
    reg_write(MmioRegister::Status, status.bits());

    // negotiate features
    let mut features = DeviceFeatures::from_bits_retain(reg_read(MmioRegister::DeviceFeatures));
    features.remove(DeviceFeatures::BLK_RO);
    features.remove(DeviceFeatures::BLK_SCSI);
    features.remove(DeviceFeatures::BLK_CONFIG_WCE);
    features.remove(DeviceFeatures::BLK_MQ);
    features.remove(DeviceFeatures::ANY_LAYOUT);
    features.remove(DeviceFeatures::RING_EVENT_IDX);
    features.remove(DeviceFeatures::RING_INDIRECT_DESC);
    reg_write(MmioRegister::DriverFeatures, features.bits());

    // tell device that feature negotiation is complete.
    status |= ConfigStatus::FEATURES_OK;
    reg_write(MmioRegister::Status, status.bits());

    // re-read status to ensure FEATURES_OK is set.
    status = ConfigStatus::from_bits_retain(reg_read(MmioRegister::Status));
    assert!(status.contains(ConfigStatus::FEATURES_OK));

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
    disk.free.fill(true);

    // tell device we're completely ready.
    status |= ConfigStatus::DRIVER_OK;
    reg_write(MmioRegister::Status, status.bits());
}

/// Finds a free descriptor, marks it non-free, returns its index.
fn alloc_desc(disk: &mut Disk) -> Option<usize> {
    let idx = disk.free.iter().position(|&free| free)?;
    disk.free[idx] = false;
    Some(idx)
}

/// Marks a descriptor as free.
fn free_desc(disk: &mut Disk, i: usize) {
    assert!(i < NUM);
    assert!(!disk.free[i]);
    unsafe {
        (*disk.desc)[i] = VirtqDesc {
            addr: 0,
            len: 0,
            flags: VirtqDescFlags::empty(),
            next: 0,
        };
        disk.free[i] = true;
    };
    proc::wakeup((&raw const disk.free[0]).cast());
}

/// Frees a chain of descriptors.
fn free_chain(disk: &mut Disk, mut i: usize) {
    loop {
        let desc = unsafe { &(*disk.desc)[i] };
        let flag = desc.flags;
        let next = desc.next;
        free_desc(disk, i);
        if !flag.contains(VirtqDescFlags::NEXT) {
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

fn read_or_write(offset: usize, data: &[u8], write: bool) {
    let mut disk = DISK.lock();

    // the spec's Section 5.2 says that legacy block operations use
    // three descriptors: one for type/reserved/sector, one for the
    // data, one for a 1-byte status result.

    // allocate three descriptors.
    let idx = loop {
        if let Some(idx) = alloc3_desc(&mut disk) {
            break idx;
        }
        proc::sleep((&raw const disk.free[0]).cast(), &mut disk);
    };

    // format the three descriptors.

    assert!(offset % BLK_SECTOR_SIZE == 0);
    let sector = (offset / BLK_SECTOR_SIZE) as u64;

    let buf0 = &mut disk.ops[idx[0]];
    *buf0 = VirtioBlkReq {
        ty: if write {
            VirtioBlkReqType::Out // write the disk
        } else {
            VirtioBlkReqType::In // read the disk
        },
        reserved: 0,
        sector,
    };
    let buf0_addr = ptr::from_mut(buf0).addr();

    unsafe {
        (*disk.desc)[idx[0]] = VirtqDesc {
            addr: buf0_addr as u64,
            len: size_of::<VirtioBlkReq>() as u32,
            flags: VirtqDescFlags::NEXT,
            next: idx[1] as u16,
        };

        (*disk.desc)[idx[1]] = VirtqDesc {
            addr: data.as_ptr().addr() as u64,
            len: BLOCK_SIZE as u32,
            flags: if write {
                VirtqDescFlags::empty() // device reads b.date
            } else {
                VirtqDescFlags::WRITE // device writes b.data
            } | VirtqDescFlags::NEXT,
            next: idx[2] as u16,
        };

        disk.info[idx[0]].status = 0xff; // device writes 0 on success
        (*disk.desc)[idx[2]] = VirtqDesc {
            addr: (&raw mut disk.info[idx[0]].status).addr() as u64,
            len: 1,
            flags: VirtqDescFlags::WRITE,
            next: 0,
        };
    }

    // record struct buf for `handle_interrupt()`.
    disk.info[idx[0]].data = data.as_ptr();
    disk.info[idx[0]].in_progress.store(true, Ordering::Release);

    // tell the device the first index in our chain of descriptors.
    unsafe {
        let avail_idx = (*disk.avail).idx.load(Ordering::Relaxed) as usize;
        (*disk.avail).ring[avail_idx % NUM] = idx[0] as u16;
    }

    // tell the device another avail ring entry is available.
    unsafe {
        (*disk.avail).idx.fetch_add(1, Ordering::AcqRel);
    }

    reg_write(MmioRegister::QueueNotify, 0); // value is queue number

    // Wait for `handle_interrupts()` to say request has finished.
    while disk.info[idx[0]].in_progress.load(Ordering::Acquire) {
        proc::sleep(data.as_ptr().cast(), &mut disk);
    }

    disk.info[idx[0]].data = ptr::null_mut();
    free_chain(&mut disk, idx[0]);
}

pub fn read(offset: usize, data: &mut [u8]) {
    read_or_write(offset, data, false);
}

pub fn write(offset: usize, data: &[u8]) {
    read_or_write(offset, data, true)
}

pub fn handle_interrupt() {
    let mut disk = DISK.lock();

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

            let info = &disk.info[id as usize];
            info.in_progress.store(false, Ordering::Release); // disk is done with buf
            proc::wakeup(info.data.cast());

            disk.used_idx += 1;
        }
    }
}
