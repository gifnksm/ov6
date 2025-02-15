//! Virtio device definitions.
//!
//! For both the MMIO interface, and Virtio descriptors.
//!
//! The Virtio spec:
//! <https://docs.oasis-open.org/virtio/virtio/v1.1/virtio-v1.1.pdf>

use core::sync::atomic::AtomicU16;

// Virtio MMIO control registers, mapped starting ad 0x1000_10000.
// from qemu virtio_mmio.h
#[repr(usize)]
pub enum MmioRegister {
    MagicValue = 0x000, // 0x74726976
    Version = 0x004,    // version; should be 2
    DeviceId = 0x008,   // device type; 1 is net, 2 is disk
    VendorId = 0x00c,   // 0x554d4551
    DeviceFeatures = 0x010,
    DriverFeatures = 0x020,
    QueueSel = 0x030,        // select queue, write-only
    QueueNumMax = 0x034,     // max size of current queue, read-only
    QueueNum = 0x038,        // size of current queue, write-only
    QueueReady = 0x044,      // ready bit
    QueueNotify = 0x050,     // write-only
    InterruptStatus = 0x060, // read-only
    InterruptAck = 0x064,    // write-only
    Status = 0x070,          // read/write
    QueueDescLow = 0x080,    // physical address for descriptor table, write-only
    QueueDescHigh = 0x084,
    DriverDescLow = 0x090, // physical address for available ring, write-only
    DriverDescHigh = 0x094,
    DeviceDescLow = 0x0a0, // physical address for used ring, write-only
    DeviceDescHigh = 0x0a4,
}

// Status register bits, from qemu virtio_config.h
pub const VIRTIO_CONFIG_S_ACKNOWLEDGE: u32 = 1;
pub const VIRTIO_CONFIG_S_DRIVER: u32 = 2;
pub const VIRTIO_CONFIG_S_DRIVER_OK: u32 = 4;
pub const VIRTIO_CONFIG_S_FEATURES_OK: u32 = 8;

// Device feature bits
pub const VIRTIO_BLK_F_RO: usize = 5; // Disk is read-only
pub const VIRTIO_BLK_F_SCSI: usize = 7; // Supports scsi command passthru
pub const VIRTIO_BLK_F_CONFIG_WCE: usize = 11; // Writeback mode available in config
pub const VIRTIO_BLK_F_MQ: usize = 12; // support more than one vq
pub const VIRTIO_F_ANY_LAYOUT: usize = 27;
pub const VIRTIO_RING_F_INDIRECT_DESC: usize = 28;
pub const VIRTIO_RING_F_EVENT_IDX: usize = 29;

// This many virtio descriptors.
// Must be a power of two.
pub const NUM: usize = 8;

// A single descriptor, from the spec.
#[repr(C)]
pub struct VirtqDesc {
    pub addr: u64,
    pub len: u32,
    pub flags: u16,
    pub next: u16,
}
pub const VRING_DESC_F_NEXT: u16 = 1; // chained with another descriptor
pub const VRING_DESC_F_WRITE: u16 = 2; // device writes (vs read)

// The (entire) avail ring, from the spec.
#[repr(C)]
pub struct VirtqAvail {
    pub flags: u16,       // always zero
    pub idx: AtomicU16,   // driver will write ring[idx] next
    pub ring: [u16; NUM], // descriptor numbers of chain heads
    pub unused: u16,
}

// One entry in the "used" ring, with which the
// device tells the driver about completed requests.
#[repr(C)]
pub struct VirtqUsedElem {
    pub id: u32, // index of start of completed descriptor chain
    pub len: u32,
}

#[repr(C)]
pub struct VirtqUsed {
    pub flags: u16,     // always zero
    pub idx: AtomicU16, // device increments when it adds a ring[] entry
    pub ring: [VirtqUsedElem; NUM],
}

// These are specific to virtio block devices, e.g. disks,
// described in Section 5.2 of the spec.

pub const VIRTIO_BLK_T_IN: u32 = 0; // read the disk
pub const VIRTIO_BLK_T_OUT: u32 = 1; // write the disk

// The format of the first descriptor in a disk request.
// to be followed by two more descriptors containing
// the block, and a one-byte status.
#[repr(C)]
pub struct VirtioBlkReq {
    pub ty: u32, // VIRTIO_BLK_T_IN or ..._OUT
    pub reserved: u32,
    pub sector: u64,
}
