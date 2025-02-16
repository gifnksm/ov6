//! Virtio device definitions.
//!
//! For both the MMIO interface, and Virtio descriptors.
//!
//! The Virtio spec:
//! <https://docs.oasis-open.org/virtio/virtio/v1.1/virtio-v1.1.pdf>

use core::sync::atomic::AtomicU16;

use bitflags::bitflags;

// Virtio MMIO control registers, mapped starting ad 0x1000_10000.
// from qemu virtio_mmio.h
#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ConfigStatus: u32 {
        const ACKNOWLEDGE = 1;
        const DRIVER = 2;
        const DRIVER_OK = 4;
        const FEATURES_OK = 8;
    }
}

// Device feature bits
bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct DeviceFeatures: u32 {
        /// Disk is read-only (`VIRTIO_BLK_F_RO`)
        const BLK_RO = 1 << 5;
        /// Supports scsi command passthru (`VIRTIO_BLK_F_SCSI`)
        const BLK_SCSI = 1 << 7;
        /// Writeback mode available in config (`VIRTIO_BLK_F_CONFIG_WCE`)
        const BLK_CONFIG_WCE = 1 << 11;
        /// support more than one vq (`VIRTIO_BLK_F_MQ`)
        const BLK_MQ = 1 << 12;
        /// `VIRTIO_F_ANY_LAYOUT`
        const ANY_LAYOUT = 1 << 27;
        /// `VIRTIO_RING_F_INDIRECT_DESC`
        const RING_INDIRECT_DESC = 1 << 28;
        /// `VIRTIO_RING_F_EVENT_IDX`
        const RING_EVENT_IDX = 1 << 29;
    }
}

// A single descriptor, from the spec.
#[repr(C)]
pub struct VirtqDesc {
    pub addr: u64,
    pub len: u32,
    pub flags: VirtqDescFlags,
    pub next: u16,
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct VirtqDescFlags: u16 {
        /// Chained with another descriptor.
        const NEXT = 1;
        /// Device writes (vs read).
        const WRITE = 2;
    }
}

// The (entire) avail ring, from the spec.
#[repr(C)]
pub struct VirtqAvail<const N: usize> {
    pub flags: u16,     // always zero
    pub idx: AtomicU16, // driver will write ring[idx] next
    pub ring: [u16; N], // descriptor numbers of chain heads
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
pub struct VirtqUsed<const N: usize> {
    pub flags: u16,     // always zero
    pub idx: AtomicU16, // device increments when it adds a ring[] entry
    pub ring: [VirtqUsedElem; N],
}

// These are specific to virtio block devices, e.g. disks,
// described in Section 5.2 of the spec.
#[repr(u32)]
pub enum VirtioBlkReqType {
    /// Read the disk
    In = 0,
    /// Write the disk
    Out = 1,
}

// The format of the first descriptor in a disk request.
// to be followed by two more descriptors containing
// the block, and a one-byte status.
#[repr(C)]
pub struct VirtioBlkReq {
    pub ty: VirtioBlkReqType,
    pub reserved: u32,
    pub sector: u64,
}

/// Sector size for virtio block devices.
pub const BLK_SECTOR_SIZE: usize = 512;
