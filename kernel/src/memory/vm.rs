use core::{
    ffi::c_void,
    mem,
    ptr::{self, NonNull},
    slice,
};

use alloc::boxed::Box;
use once_init::OnceInit;
use riscv::{asm, register::satp};

use crate::{
    error::Error,
    interrupt::trampoline,
    memory::{
        PAGE_SIZE, PageRound as _, PhysAddr, VirtAddr,
        layout::{KERN_BASE, PHYS_TOP, PLIC, TRAMPOLINE, UART0, VIRTIO0},
        page::{self, PageFrameAllocator},
        page_table::{PageTable, PtEntryFlags},
    },
    proc,
};

/// The kernel's page table address.
static KERNEL_PAGETABLE: OnceInit<Box<PageTable, PageFrameAllocator>> = OnceInit::new();

/// Address of the end of kernel code.
const ETEXT: NonNull<c_void> = {
    unsafe extern "C" {
        #[link_name = "etext"]
        static mut ETEXT: [u8; 0];
    }
    NonNull::new((&raw mut ETEXT).cast()).unwrap()
};

/// Makes a direct-map page table for the kernel.
fn make_kernel_pt() -> Box<PageTable, PageFrameAllocator> {
    use PtEntryFlags as F;

    let etext = ETEXT.addr().into();
    let phys_trampoline = PhysAddr::new(trampoline::trampoline as usize);

    unsafe fn ident_map(
        kpgtbl: &mut PageTable,
        addr: usize,
        size: usize,
        perm: PtEntryFlags,
    ) -> Result<(), Error> {
        kpgtbl.map_pages(VirtAddr::new(addr), size, PhysAddr::new(addr), perm)
    }

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
        ident_map(&mut kpgtbl, KERN_BASE, etext - KERN_BASE, rx).unwrap();

        // map kernel data and the physical RAM we'll make use of.
        ident_map(&mut kpgtbl, etext, PHYS_TOP - etext, rw).unwrap();

        // map the trampoline for trap entry/exit to
        // the highest virtual address in the kernel.
        kpgtbl
            .map_pages(TRAMPOLINE, PAGE_SIZE, phys_trampoline, rx)
            .unwrap();

        // allocate and map a kernel stack for each process.
        proc::map_stacks(&mut kpgtbl);
    }

    kpgtbl
}

pub mod kernel {
    use super::*;

    /// Initialize the one kernel_pagetable
    pub fn init() {
        let kpgtbl = make_kernel_pt();
        KERNEL_PAGETABLE.init(kpgtbl);
    }

    /// Switch h/w page table register to the kernel's page table,
    /// and enable paging.
    pub fn init_hart() {
        // wait for any previous writes to the page table memory to finish.
        asm::sfence_vma_all();

        let addr = KERNEL_PAGETABLE.get().phys_addr();
        unsafe {
            satp::set(satp::Mode::Sv39, 0, addr.phys_page_num().value());
        }

        // flush state entries from the TLB.
        asm::sfence_vma_all();
    }
}

pub mod user {
    use core::slice;

    use super::*;

    /// Removes npages of mappings starting from `va``.
    ///
    /// `va`` must be page-aligned.
    /// The mappings must exist.
    ///
    /// Optionally free the physical memory.
    pub fn unmap(pagetable: &mut PageTable, va: VirtAddr, npages: usize, do_free: bool) {
        for pa in pagetable.unmap_pages(va, npages) {
            if do_free {
                unsafe {
                    page::free_page(pa.as_mut_ptr());
                }
            }
        }
    }

    /// Creates an empty user page table.
    ///
    /// Returns `Err()` if out of memory.
    pub fn create() -> Result<Box<PageTable, PageFrameAllocator>, Error> {
        PageTable::try_allocate()
    }

    /// Loads the user initcode into address 0 of pagetable.
    ///
    /// For the very first process.
    /// `sz` must be less than a page.
    pub fn map_first(pagetable: &mut PageTable, src: &[u8]) {
        assert!(src.len() < PAGE_SIZE, "src.len()={:#x}", src.len());

        unsafe {
            let mem = page::alloc_zeroed_page().unwrap();
            pagetable
                .map_page(
                    VirtAddr::new(0),
                    PhysAddr::new(mem.addr().get()),
                    PtEntryFlags::URWX,
                )
                .unwrap();
            slice::from_raw_parts_mut(mem.as_ptr(), src.len()).copy_from_slice(src);
        }
    }

    /// Allocates PTEs and physical memory to grow process from `oldsz` to `newsz`,
    /// which need not be page aligned.
    ///
    /// Returns new size.
    pub fn alloc(
        pagetable: &mut PageTable,
        oldsz: usize,
        newsz: usize,
        xperm: PtEntryFlags,
    ) -> Result<usize, Error> {
        if newsz < oldsz {
            return Ok(oldsz);
        }

        let oldsz = oldsz.page_roundup();
        for va in (oldsz..newsz).step_by(PAGE_SIZE) {
            let Some(mem) = page::alloc_zeroed_page() else {
                dealloc(pagetable, va, oldsz);
                return Err(Error::Unknown);
            };
            if pagetable
                .map_page(
                    VirtAddr::new(va),
                    PhysAddr::new(mem.addr().get()),
                    xperm | PtEntryFlags::UR,
                )
                .is_err()
            {
                unsafe {
                    page::free_page(mem);
                }
                dealloc(pagetable, va, oldsz);
                return Err(Error::Unknown);
            }
        }

        Ok(newsz)
    }

    /// Deallocates user pages to bring the process size from `oldsz` to `newsz`.
    ///
    /// `oldsz` and `newsz` need not be page-aligned, nor does `newsz`
    /// need to be less than `oldsz`.
    /// `oldsz` can be larger than the acrual process size.
    ///
    /// Returns the new process size.
    pub fn dealloc(pagetable: &mut PageTable, oldsz: usize, newsz: usize) -> usize {
        if newsz >= oldsz {
            return oldsz;
        }

        if newsz.page_roundup() < oldsz.page_roundup() {
            let npages = (oldsz.page_roundup() - newsz.page_roundup()) / PAGE_SIZE;
            unmap(pagetable, VirtAddr::new(newsz.page_roundup()), npages, true);
        }

        newsz
    }

    /// Frees user memory pages, then free page-table pages.
    pub fn free(mut pagetable: Box<PageTable, PageFrameAllocator>, sz: usize) {
        if sz > 0 {
            unmap(
                &mut pagetable,
                VirtAddr::new(0),
                sz.page_roundup() / PAGE_SIZE,
                true,
            );
        }
        pagetable.free_descendant();
    }

    /// Given a parent process's page table, copies
    /// its memory into a child's page table.
    ///
    /// Copies both the page table and the
    /// physical memory.
    pub fn copy(old: &PageTable, new: &mut PageTable, sz: usize) -> Result<(), Error> {
        let res = (|| {
            for va in (0..sz).step_by(PAGE_SIZE) {
                let pte = old.find_leaf_entry(VirtAddr::new(va)).ok_or(va)?;
                assert!(pte.is_valid() && pte.is_leaf());
                let src_pa = pte.phys_addr();
                let flags = pte.flags();
                let Some(dst) = page::alloc_page() else {
                    return Err(va);
                };
                unsafe {
                    dst.as_ptr().copy_from(src_pa.as_ptr(), PAGE_SIZE);
                }
                if new
                    .map_page(VirtAddr::new(va), PhysAddr::new(dst.addr().get()), flags)
                    .is_err()
                {
                    return Err(va);
                }
            }
            Ok(())
        })();

        if let Err(va) = res {
            unmap(new, VirtAddr::new(0), va / PAGE_SIZE, true);
        }

        res.map_err(|_| Error::Unknown)
    }

    /// Marks a PTE invalid for user access.
    ///
    /// Used by exec for the user stackguard page.
    pub fn forbide_user_access(pagetable: &mut PageTable, va: VirtAddr) {
        pagetable
            .update_level0_entry(va, false, |pte| {
                let mut flags = pte.flags();
                flags.remove(PtEntryFlags::U);
                pte.set_flags(flags);
            })
            .unwrap();
    }
}

/// Copies from user to kernel.
///
/// Copies from `src` to virtual address `dst_va` in a given page table.
pub fn copy_out<T>(pagetable: &PageTable, dst_va: VirtAddr, src: &T) -> Result<(), Error> {
    let src = unsafe { slice::from_raw_parts(ptr::from_ref(src).cast(), mem::size_of::<T>()) };
    copy_out_bytes(pagetable, dst_va, src)
}

/// Copies from kernel to user.
///
/// Copies from `src` to virtual address `dst_va` in a given page table.
pub fn copy_out_bytes(
    pagetable: &PageTable,
    mut dst_va: VirtAddr,
    mut src: &[u8],
) -> Result<(), Error> {
    while !src.is_empty() {
        let va0 = dst_va.page_rounddown();
        if va0 >= VirtAddr::MAX {
            return Err(Error::Unknown);
        }
        let offset = dst_va.addr() - va0.addr();
        let mut n = PAGE_SIZE - offset;
        if n > src.len() {
            n = src.len();
        }

        let dst_page = pagetable
            .fetch_page(va0, PtEntryFlags::UW)
            .ok_or(Error::Unknown)?;
        let dst = &mut dst_page[offset..][..n];
        dst.copy_from_slice(&src[..n]);
        src = &src[n..];
        dst_va = va0.byte_add(PAGE_SIZE);
    }

    Ok(())
}

/// Copies from user to kernel.
///
/// Returns the copy from virtual address `src_va` in a given page table.
pub fn copy_in<T>(pagetable: &PageTable, src_va: VirtAddr) -> Result<T, Error> {
    let mut dst = mem::MaybeUninit::<T>::uninit();
    copy_in_raw(pagetable, dst.as_mut_ptr().cast(), size_of::<T>(), src_va)?;
    Ok(unsafe { dst.assume_init() })
}

// /// Copies from user to kernel.
// ///
// /// Copies to `dst` from virtual address `src_va` in a given page table.
// pub fn copy_in_to<T>(pagetable: &PageTable, dst: &mut T, src_va: VirtAddr) -> Result<(), Error> {
//     copy_in_raw(pagetable, ptr::from_mut(dst).cast(), size_of::<T>(), src_va)
// }

/// Copies from user to kernel.
///
/// Copies to `dst` from virtual address `src_va` in a given page table.
pub fn copy_in_bytes(pagetable: &PageTable, dst: &mut [u8], src_va: VirtAddr) -> Result<(), Error> {
    copy_in_raw(pagetable, dst.as_mut_ptr(), dst.len(), src_va)
}

/// Copies from user to kernel.
///
/// Copies to `dst` from virtual address `src_va` in a given page table.
pub fn copy_in_raw(
    pagetable: &PageTable,
    mut dst: *mut u8,
    mut dst_size: usize,
    mut src_va: VirtAddr,
) -> Result<(), Error> {
    while dst_size > 0 {
        let va0 = src_va.page_rounddown();
        let offset = src_va.addr() - va0.addr();
        let mut n = PAGE_SIZE - offset;
        if n > dst_size {
            n = dst_size;
        }
        let src_page = pagetable
            .fetch_page(va0, PtEntryFlags::UR)
            .ok_or(Error::Unknown)?;
        let src = &src_page[offset..][..n];
        unsafe {
            dst.copy_from(src.as_ptr(), n);
            dst = dst.add(n);
            dst_size -= n;
        }
        src_va = va0.byte_add(PAGE_SIZE);
    }

    Ok(())
}

/// Copies a null-terminated string from user to kernel.
///
/// Copies bytes to `dst` from virtual address `src_va` in a given page table,
/// until a '\0', or max.
pub fn copy_in_str(
    pagetable: &PageTable,
    mut dst: &mut [u8],
    mut src_va: VirtAddr,
) -> Result<(), Error> {
    while !dst.is_empty() {
        let va0 = src_va.page_rounddown();
        let src_page = pagetable
            .fetch_page(va0, PtEntryFlags::UR)
            .ok_or(Error::Unknown)?;

        let offset = src_va.addr() - va0.addr();
        let mut n = PAGE_SIZE - offset;
        if n > dst.len() {
            n = dst.len();
        }

        let mut p = &src_page[offset..];
        while n > 0 {
            if p[0] == b'\0' {
                dst[0] = b'\0';
                return Ok(());
            }
            dst[0] = p[0];
            n -= 1;
            p = &p[1..];
            dst = &mut dst[1..];
        }

        src_va = va0.byte_add(PAGE_SIZE);
    }
    Err(Error::Unknown)
}
