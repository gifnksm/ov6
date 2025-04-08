use once_init::OnceInit;
use ov6_kernel_params::NPROC;
use riscv::{asm, register::satp};

use super::{
    layout::{self, KSTACK_PAGES},
    page_table::{self, MapTarget, PageTable},
};
use crate::{
    error::KernelError,
    interrupt::trampoline,
    memory::{
        PAGE_SIZE, PhysAddr, VirtAddr,
        layout::{
            CLINT, CLINT_SIZE, KERNEL_BASE, PHYS_TOP, PLIC, PLIC_SIZE, TEXT_END, TRAMPOLINE, UART0,
            VIRT_TEST, VIRTIO0,
        },
        page_table::PtEntryFlags,
    },
};

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

    let satp = KERNEL_PAGE_TABLE.get().0.satp();
    unsafe {
        satp::write(satp);
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
    unsafe {
        kpgtbl.map_addrs(
            VirtAddr::new(addr)?,
            MapTarget::fixed_addr(PhysAddr::new(addr)),
            size,
            perm,
        )
    }
}

pub struct KernelPageTable(PageTable);

impl KernelPageTable {
    /// Makes a direct-map page table for the kernel.
    pub fn new() -> Self {
        use PtEntryFlags as F;

        let phys_trampoline = PhysAddr::new(trampoline::trampoline as usize);

        let rw = F::RW;
        let rx = F::RX;

        let mut kpgtbl = PageTable::try_allocate().unwrap();

        unsafe {
            // SiFive test MMIO device
            ident_map(&mut kpgtbl, VIRT_TEST, PAGE_SIZE, rw).unwrap();

            // uart registers
            ident_map(&mut kpgtbl, UART0, PAGE_SIZE, rw).unwrap();

            // virtio mmio disk interface
            ident_map(&mut kpgtbl, VIRTIO0, PAGE_SIZE, rw).unwrap();

            // CLINT
            ident_map(&mut kpgtbl, CLINT, CLINT_SIZE, rw).unwrap();

            // PLIC
            ident_map(&mut kpgtbl, PLIC, PLIC_SIZE, rw).unwrap();

            // map kernel text executable and red-only.
            ident_map(&mut kpgtbl, KERNEL_BASE, TEXT_END - KERNEL_BASE, rx).unwrap();

            // map kernel data and the physical RAM we'll make use of.
            ident_map(&mut kpgtbl, TEXT_END, PHYS_TOP - TEXT_END, rw).unwrap();

            // map the trampoline for trap entry/exit to
            // the highest virtual address in the kernel.
            kpgtbl
                .map_addrs(
                    TRAMPOLINE,
                    MapTarget::fixed_addr(phys_trampoline),
                    PAGE_SIZE,
                    rx,
                )
                .unwrap();

            // allocate and map a kernel stack for each process.
            map_proc_stacks(&mut kpgtbl);
        }

        Self(kpgtbl)
    }
}

fn map_proc_stacks(kpgtbl: &mut PageTable) {
    for i in (0..NPROC).rev() {
        let va = layout::kstack(i);
        unsafe {
            kpgtbl.map_addrs(
                va,
                MapTarget::allocate_new_zeroed(),
                KSTACK_PAGES * PAGE_SIZE,
                PtEntryFlags::RW,
            )
        }
        .unwrap();
    }
}

pub(crate) fn dump() {
    let kpgtbl = KERNEL_PAGE_TABLE.get();
    page_table::dump_pagetable(&kpgtbl.0);
}
