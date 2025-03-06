use alloc::boxed::Box;
use once_init::OnceInit;
use riscv::{asm, register::satp};

use crate::{
    error::KernelError,
    interrupt::trampoline,
    memory::{
        PAGE_SIZE, PhysAddr, VirtAddr,
        layout::{KERN_BASE, PHYS_TOP, PLIC, TEXT_END, TRAMPOLINE, UART0, VIRTIO0},
        page_table::PtEntryFlags,
    },
    proc,
};

use super::{page::PageFrameAllocator, page_table::PageTable};

/// The kernel's page table address.
static KERNEL_PAGE_TABLE: OnceInit<KernelPageTable> = OnceInit::new();

/// Initialize the one `KernelPageTable`
pub fn init() {
    KERNEL_PAGE_TABLE.init(KernelPageTable::new());
}

/// Switch h/w page table register to the kernel's page table,
/// and enable paging.
pub fn init_hart() {
    // wait for any previous writes to the page table memory to finish.
    asm::sfence_vma_all();

    let addr = KERNEL_PAGE_TABLE.get().0.phys_addr();
    unsafe {
        satp::set(satp::Mode::Sv39, 0, addr.phys_page_num().value());
    }

    // flush state entries from the TLB.
    asm::sfence_vma_all();
}

unsafe fn ident_map(
    kpgtbl: &mut PageTable,
    addr: usize,
    size: usize,
    perm: PtEntryFlags,
) -> Result<(), KernelError> {
    kpgtbl.map_pages(VirtAddr::new(addr), size, PhysAddr::new(addr), perm)
}

pub struct KernelPageTable(Box<PageTable, PageFrameAllocator>);

impl KernelPageTable {
    /// Makes a direct-map page table for the kernel.
    pub fn new() -> Self {
        use PtEntryFlags as F;

        let phys_trampoline = PhysAddr::new(trampoline::trampoline as usize);

        let rw = F::RW;
        let rx = F::RX;

        let mut kpgtbl = PageTable::try_allocate().unwrap();

        unsafe {
            // uart registers
            ident_map(&mut kpgtbl, UART0, PAGE_SIZE, rw).unwrap();

            // virtio mmio disk interface
            ident_map(&mut kpgtbl, VIRTIO0, PAGE_SIZE, rw).unwrap();

            // PLIC
            ident_map(&mut kpgtbl, PLIC, 0x400_0000, rw).unwrap();

            // map kernel text executable and red-only.
            ident_map(&mut kpgtbl, KERN_BASE, TEXT_END - KERN_BASE, rx).unwrap();

            // map kernel data and the physical RAM we'll make use of.
            ident_map(&mut kpgtbl, TEXT_END, PHYS_TOP - TEXT_END, rw).unwrap();

            // map the trampoline for trap entry/exit to
            // the highest virtual address in the kernel.
            kpgtbl
                .map_pages(TRAMPOLINE, PAGE_SIZE, phys_trampoline, rx)
                .unwrap();

            // allocate and map a kernel stack for each process.
            proc::map_stacks(&mut kpgtbl);
        }

        Self(kpgtbl)
    }
}
