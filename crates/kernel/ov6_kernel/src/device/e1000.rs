use alloc::boxed::Box;
use core::{array, pin::Pin, ptr};

use bitflags::{Flags as _, bitflags};
use dataview::{Pod, PodMethods as _};
use once_init::OnceInit;
use safe_cast::{SafeFrom as _, SafeInto as _};

use crate::{
    memory::{PAGE_SIZE, page::PageFrameAllocator},
    net,
    sync::{SpinLock, SpinLockGuard},
};

pub(crate) unsafe fn init(regs: *mut u32) {
    #[expect(clippy::enum_glob_use)]
    use Register::*;

    let rx_bufs = array::from_fn(|_| {
        Some(Box::into_pin(unsafe {
            Box::<[u8; PAGE_SIZE], _>::new_zeroed_in(PageFrameAllocator).assume_init()
        }))
    });
    let tx_bufs = array::from_fn(|_| {
        Box::into_pin(unsafe {
            Box::<[u8; PAGE_SIZE], _>::new_zeroed_in(PageFrameAllocator).assume_init()
        })
    });

    DRIVER.init(SpinLock::new(Driver {
        registers: regs,
        rx_ring: RxRing(array::from_fn(|i| RxDesc {
            addr: rx_bufs[i].as_ref().unwrap().as_ptr().addr().safe_into(),
            ..RxDesc::zeroed()
        })),
        rx_bufs,
        tx_ring: TxRing(array::from_fn(|i| TxDesc {
            addr: tx_bufs[i].as_ptr().addr().safe_into(),
            status: TxdStat::Dd,
            ..TxDesc::zeroed()
        })),
        tx_bufs,
    }));

    let mut driver = DRIVER.get().lock();

    // Reset the device
    unsafe {
        // disable interrupts
        driver.write_reg(Ims, 0);
        driver.set_reg_flag(Ctl, CtlBits::RST.bits());
        // re-enable interrupts
        driver.write_reg(Ims, 0);
    }

    // [E1000 14.5] Transmit initialization
    unsafe {
        let tx_ring_addr = ptr::from_ref(&driver.tx_ring).addr();
        let tx_ring_size = size_of_val(&driver.tx_ring);
        assert!(tx_ring_addr.trailing_zeros() >= 4);
        driver.write_reg(TdbaL, (tx_ring_addr & 0xffff_ffff).try_into().unwrap());
        driver.write_reg(TdbaH, (tx_ring_addr >> 32).try_into().unwrap());
        assert_eq!(size_of::<TxRing>() % 128, 0);
        driver.write_reg(Tdlen, tx_ring_size.try_into().unwrap());
        driver.write_reg(Tdh, 0);
        driver.write_reg(Tdt, 0);
    }

    // [E1000 14.4] Receive initialization
    unsafe {
        let rx_ring_addr = ptr::from_ref(&driver.rx_ring).addr();
        let rx_ring_size = size_of_val(&driver.rx_ring);
        assert!(rx_ring_addr.trailing_zeros() >= 4);
        driver.write_reg(RdbaL, (rx_ring_addr & 0xffff_ffff).try_into().unwrap());
        driver.write_reg(RdbaH, (rx_ring_addr >> 32).try_into().unwrap());
        assert_eq!(size_of::<RxRing>() % 128, 0);
        driver.write_reg(Rdh, 0);
        driver.write_reg(Rdt, (RX_RING_SIZE - 1).try_into().unwrap());
        driver.write_reg(Rdlen, rx_ring_size.try_into().unwrap());
    }

    // filter by qemu's MAC address, 52:54:00:12:34:56
    unsafe {
        driver.write_reg_array(Ra, 0, 0x1200_5452);
        driver.write_reg_array(Ra, 1, 0x5634 | (1 << 31));
        // multicast table
        for i in 0..4096 / 32 {
            driver.write_reg_array(Mta, i, 0);
        }
    }

    // transmitter control bits.
    unsafe {
        Tctl.write(
            regs,
            (
                // enable
                TctlBits::EN  |
                // pad short packets
                TctlBits::PSP
            )
                .bits()
                | (
                    // collision stuff
                    (0x10 << TCTL_CT_SHIFT) | (0x40 << TCTL_COLD_SHIFT)
                ),
        );
        // inter-pkg gap
        driver.write_reg(Tipg, 10 | (8 << 10) | (6 << 20));
    }

    // receiver control bits.
    unsafe {
        Rctl.write(
            regs,
            (
                // enable receiver
                RctlBits::EN |
                // enable broadcast
                RctlBits::BAM |
                // 2048-byte rx buffers
                RctlBits::SZ_2048 |
                // strip CRC
                RctlBits::SECRC
            )
                .bits(),
        );
    }

    // ask e1000 for receive interrupts.
    unsafe {
        // interrupt after every received packet (no timer)
        driver.write_reg(Rdtr, 0);
        // interrupt after every packet (no timer)
        driver.write_reg(Radv, 0);
        // RXDW -- Receiver Descriptor Write Back
        driver.write_reg(Ims, 1 << 7);
    }
}

pub fn handle_interrupt() {
    let mut driver = DRIVER.get().lock();
    unsafe {
        driver.write_reg(Register::Icr, u32::MAX);
    }
    receive(driver);
}

pub fn transmitter() -> Option<Transmitter<'static>> {
    let mut driver = DRIVER.get().lock();
    let tail = unsafe { driver.read_reg(Register::Tdt) };
    let index = usize::safe_from(tail);
    let desc = &mut driver.tx_ring.0[index];
    if !desc.status.contains(TxdStat::Dd) {
        return None;
    }
    desc.length = 0;
    driver.tx_bufs[index].fill(0);
    Some(Transmitter { driver, index })
}

pub struct Transmitter<'a> {
    driver: SpinLockGuard<'a, Driver>,
    index: usize,
}

impl Transmitter<'_> {
    pub fn buffer(&mut self) -> &mut [u8] {
        self.driver.tx_bufs[self.index].as_mut_slice()
    }

    pub fn set_len(&mut self, len: usize) {
        self.driver.tx_ring.0[self.index].length = u16::try_from(len).unwrap();
    }

    pub fn send(mut self) {
        let desc = &mut self.driver.tx_ring.0[self.index];
        assert!(desc.length > 0);
        desc.cmd = TxdCmd::Rs | TxdCmd::Eop;
        unsafe {
            self.driver.write_reg(
                Register::Tdt,
                u32::try_from((self.index + 1) % TX_RING_SIZE).unwrap(),
            );
        }
    }
}

fn receive(mut driver: SpinLockGuard<'_, Driver>) -> SpinLockGuard<'_, Driver> {
    loop {
        let tail = unsafe { driver.read_reg(Register::Rdt) };
        let index = (usize::safe_from(tail) + 1) % RX_RING_SIZE;
        let desc = &mut driver.rx_ring.0[index];
        if !desc.status.contains(RxdStat::Dd) {
            return driver;
        }
        let length = desc.length;

        let mut buf = driver.rx_bufs[index].take().unwrap();
        drop(driver);
        net::handle_receive(&buf[..usize::from(length)]);
        buf.fill(0);

        driver = DRIVER.get().lock();
        driver.rx_bufs[index] = Some(buf);
        let desc = &mut driver.rx_ring.0[index];
        desc.status.clear();
        unsafe {
            driver.write_reg(Register::Rdt, u32::try_from(index).unwrap());
        }
    }
}

const TX_RING_SIZE: usize = 16;

#[repr(align(16), C)]
#[derive(Pod)]
struct TxRing([TxDesc; TX_RING_SIZE]);

const RX_RING_SIZE: usize = 16;

#[repr(align(16), C)]
#[derive(Pod)]
struct RxRing([RxDesc; RX_RING_SIZE]);

type Buf = Pin<Box<[u8; PAGE_SIZE], PageFrameAllocator>>;

struct Driver {
    registers: *mut u32,
    rx_ring: RxRing,
    rx_bufs: [Option<Buf>; RX_RING_SIZE],
    tx_ring: TxRing,
    tx_bufs: [Buf; TX_RING_SIZE],
}

unsafe impl Send for Driver {}

static DRIVER: OnceInit<SpinLock<Driver>> = OnceInit::new();

impl Driver {
    unsafe fn read_reg(&mut self, reg: Register) -> u32 {
        unsafe { reg.read(self.registers) }
    }

    unsafe fn set_reg_flag(&mut self, reg: Register, flags: u32) {
        unsafe {
            reg.set_flags(self.registers, flags);
        }
    }

    unsafe fn write_reg(&mut self, reg: Register, value: u32) {
        unsafe { reg.write(self.registers, value) }
    }

    unsafe fn write_reg_array(&mut self, reg: Register, index: usize, value: u32) {
        unsafe { reg.write_array(self.registers, index, value) }
    }
}

/// Device Control Register - RW
#[derive(Clone, Copy)]
#[repr(usize)]
#[expect(dead_code)]
enum Register {
    /// Device Control Register - RW
    Ctl = 0x00000,
    /// Interrupt Cause Read - R
    Icr = 0x000C0,
    /// Interrupt Mask Set - RW
    Ims = 0x000D0,
    /// RX Control - RW
    Rctl = 0x00100,
    /// TX Control - RW
    Tctl = 0x00400,
    /// TX Inter-packet gap - RW
    Tipg = 0x00410,
    /// RX Descriptor Base Address Low - RW
    RdbaL = 0x02800,
    /// RX Descriptor Base Address High - RW
    RdbaH = 0x02804,
    /// RX Delay Timer
    Rdtr = 0x02820,
    /// RX Interrupt Absolute Delay Timer
    Radv = 0x0282C,
    /// RX Descriptor Head - RW
    Rdh = 0x02810,
    /// RX Descriptor Tail - RW
    Rdt = 0x02818,
    /// RX Descriptor Length - RW
    Rdlen = 0x02808,
    /// RX Small Packet Detect Interrupt
    Rsprpd = 0x02C00,
    /// TX Descriptor Base Address Low - RW
    TdbaL = 0x03800,
    /// TX Descriptor Base Address High - RW
    TdbaH = 0x03804,
    /// TX Descriptor Length - RW
    Tdlen = 0x03808,
    /// TX Descriptor Head - RW
    Tdh = 0x03810,
    /// TX Descriptor Tail - RW
    Tdt = 0x03818,
    /// Multicast Table Array - RW Array
    Mta = 0x05200,
    /// Receive Address - RW Array
    Ra = 0x05400,
}

impl Register {
    unsafe fn read(self, base: *const u32) -> u32 {
        unsafe { base.add(self as usize / 4).read_volatile() }
    }

    unsafe fn write(self, base: *mut u32, value: u32) {
        unsafe { base.add(self as usize / 4).write_volatile(value) }
    }

    unsafe fn write_array(self, base: *mut u32, index: usize, value: u32) {
        unsafe {
            base.add(self as usize / 4).add(index).write_volatile(value);
        }
    }

    unsafe fn set_flags(self, base: *mut u32, flags: u32) {
        unsafe {
            let old = self.read(base);
            self.write(base, old | flags);
        }
    }
}

bitflags! {
    /// Device Control Register values
    #[repr(transparent)]
    #[derive(Clone, Copy)]
    struct CtlBits: u32 {
        /// set link up
        const SLU =      0x0000_0040;
        /// force speed
        const FRCSPD =   0x0000_0800;
        /// force duplex
        const FRCDPLX =  0x0000_1000;
        /// full reset
        const RST =      0x0400_0000;
    }
}

bitflags! {
    /// Transmit Control Register Values
    #[repr(transparent)]
    #[derive(Clone, Copy)]
    struct TctlBits: u32 {
        /// software reset
        const RST    = 0x0000_0001;
        /// enable tx
        const EN     = 0x0000_0002;
        /// busy check enable
        const BCE    = 0x0000_0004;
        /// pad short packets
        const PSP    = 0x0000_0008;
        /// collision threshold
        const CT     = 0x0000_0ff0;
        /// collision distance
        const COLD   = 0x003f_f000;
        /// SW Xoff transmission
        const SWXOFF = 0x0040_0000;
        /// Packet Burst Enable
        const PBE    = 0x0080_0000;
        /// Re-transmit on late collision
        const RTLC   = 0x0100_0000;
        /// No Re-transmit on underrun
        const NRTU   = 0x0200_0000;
        /// Multiple request support
        const MULR   = 0x1000_0000;
    }
}

const TCTL_CT_SHIFT: usize = 4;
const TCTL_COLD_SHIFT: usize = 12;

bitflags! {
    /// Receive Control Register Values
    #[repr(transparent)]
    #[derive(Clone, Copy)]
    struct RctlBits: u32 {
        ///  Software reset */
        const RST =             0x0000_0001;
        ///  enable */
        const EN =              0x0000_0002;
        ///  store bad packet */
        const SBP =             0x0000_0004;
        ///  unicast promiscuous enable */
        const UPE =             0x0000_0008;
        ///  multicast promiscuous enab */
        const MPE =             0x0000_0010;
        ///  long packet enable */
        const LPE =             0x0000_0020;
        ///  no loopback mode */
        const LBM_NO =          0x0000_0000;
        ///  MAC loopback mode */
        const LBM_MAC =         0x0000_0040;
        ///  serial link loopback mode */
        const LBM_SLP =         0x0000_0080;
        ///  tcvr loopback mode */
        const LBM_TCVR =        0x0000_00C0;
        ///  Descriptor type mask */
        const DTYP_MASK =       0x0000_0C00;
        ///  Packet Split descriptor */
        const DTYP_PS =         0x0000_0400;
        ///  rx desc min threshold size */
        const RDMTS_HALF =      0x0000_0000;
        ///  rx desc min threshold size */
        const RDMTS_QUAT =      0x0000_0100;
        ///  rx desc min threshold size */
        const RDMTS_EIGHTH =    0x0000_0200;
        ///  multicast offset 11:0 */
        const MO_0 =            0x0000_0000;
        ///  multicast offset 12:1 */
        const MO_1 =            0x0000_1000;
        ///  multicast offset 13:2 */
        const MO_2 =            0x0000_2000;
        ///  multicast offset 15:4 */
        const MO_3 =            0x0000_3000;
        ///  multicast desc ring 0 */
        const MDR =             0x0000_4000;
        ///  broadcast enable */
        const BAM =             0x0000_8000;
        /* these buffer sizes are valid if E1000_RCTL_BSEX is 0 */
        ///  rx buffer size 2048 */
        const SZ_2048 =         0x0000_0000;
        ///  rx buffer size 1024 */
        const SZ_1024 =         0x0001_0000;
        ///  rx buffer size 512 */
        const SZ_512 =          0x0002_0000;
        ///  rx buffer size 256 */
        const SZ_256 =          0x0003_0000;
        /* these buffer sizes are valid if E1000_RCTL_BSEX is 1 */
        ///  rx buffer size 16384 */
        const SZ_16384 =        0x0001_0000;
        ///  rx buffer size 8192 */
        const SZ_8192 =         0x0002_0000;
        ///  rx buffer size 4096 */
        const SZ_4096 =         0x0003_0000;
        ///  vlan filter enable */
        const VFE =             0x0004_0000;
        ///  canonical form enable */
        const CFIEN =           0x0008_0000;
        ///  canonical form indicator */
        const CFI =             0x0010_0000;
        ///  discard pause frames */
        const DPF =             0x0040_0000;
        ///  pass MAC control frames */
        const PMCF =            0x0080_0000;
        ///  Buffer size extension */
        const BSEX =            0x0200_0000;
        ///  Strip Ethernet CRC */
        const SECRC =           0x0400_0000;
        ///  Flexible buffer size */
        const FLXBUF_MASK =     0x7800_0000;
    }
}

bitflags! {
    /// Transmit Descriptor command definitions [E1000 3.3.3.1]
    #[repr(transparent)]
    #[derive(Debug, Clone, Copy)]
    struct TxdCmd: u8 {
        /// End of Packet
        const Eop = 0x01;
        /// Report Status
        const Rs = 0x08;
    }
}

unsafe impl Pod for TxdCmd {}

bitflags! {
    /// Transmit Descriptor status definitions [E1000 3.3.3.2]
    #[repr(transparent)]
    #[derive(Debug, Clone, Copy)]
    struct TxdStat: u8 {
        /// Descriptor Done
        const Dd = 0x01;
    }
}

unsafe impl Pod for TxdStat {}

/// Legacy Transmit Descriptor Format [E1000 3.3.3]
#[repr(C)]
#[derive(Debug, Clone, Pod)]
struct TxDesc {
    /// Buffer Address
    addr: u64,
    /// Per segment length
    length: u16,
    /// Checksum Offset
    cso: u8,
    /// Command field
    cmd: TxdCmd,
    /// Status field
    status: TxdStat,
    /// Checksum Start Field
    css: u8,
    /// Special Field
    special: u16,
}

bitflags! {
    /// Receive Descriptor bit definitions [E1000 3.2.3.1]
    #[repr(transparent)]
    #[derive(Debug, Clone, Copy)]
    struct RxdStat: u8 {
        /// Descriptor Done
        const Dd = 0x01;
        /// End of Packet
        const Eop = 0x02;
    }
}

unsafe impl Pod for RxdStat {}

/// Receive Descriptor Format [E1000 3.2.3]
#[derive(Debug, Clone, Pod)]
#[repr(C)]
struct RxDesc {
    /// Address of the descriptor's data buffer
    addr: u64,
    /// Length of data DMA-ed into data buffer
    length: u16,
    /// Packet checksum
    csum: u16,
    /// Descriptor status
    status: RxdStat,
    /// Descriptor errors
    errors: u8,
    /// Special field
    special: u16,
}
