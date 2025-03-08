use core::{mem, ptr, slice};

use super::user::UserPageTable;
use crate::{
    error::KernelError,
    memory::{PAGE_SIZE, PageRound as _, VirtAddr, page_table::PtEntryFlags},
};

/// Copies from user to kernel.
///
/// Copies from `src` to virtual address `dst_va` in a given page table.
pub fn copy_out<T>(
    pagetable: &mut UserPageTable,
    dst_va: VirtAddr,
    src: &T,
) -> Result<(), KernelError> {
    let src = unsafe { slice::from_raw_parts(ptr::from_ref(src).cast(), mem::size_of::<T>()) };
    copy_out_bytes(pagetable, dst_va, src)
}

/// Copies from kernel to user.
///
/// Copies from `src` to virtual address `dst_va` in a given page table.
pub fn copy_out_bytes(
    pagetable: &mut UserPageTable,
    mut dst_va: VirtAddr,
    mut src: &[u8],
) -> Result<(), KernelError> {
    while !src.is_empty() {
        let va0 = dst_va.page_rounddown();
        if va0 >= VirtAddr::MAX {
            return Err(KernelError::Unknown);
        }
        let offset = dst_va.addr() - va0.addr();
        let mut n = PAGE_SIZE - offset;
        if n > src.len() {
            n = src.len();
        }

        let dst_page = pagetable.fetch_page_mut(va0, PtEntryFlags::UW)?;
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
pub fn copy_in<T>(pagetable: &UserPageTable, src_va: VirtAddr) -> Result<T, KernelError> {
    let mut dst = mem::MaybeUninit::<T>::uninit();
    copy_in_raw(pagetable, dst.as_mut_ptr().cast(), size_of::<T>(), src_va)?;
    Ok(unsafe { dst.assume_init() })
}

// /// Copies from user to kernel.
// ///
// /// Copies to `dst` from virtual address `src_va` in a given page table.
// pub fn copy_in_to<T>(pagetable: &mut UserPageTable, dst: &mut T, src_va:
// VirtAddr) -> Result<(), Error> {     copy_in_raw(pagetable,
// ptr::from_mut(dst).cast(), size_of::<T>(), src_va) }

/// Copies from user to kernel.
///
/// Copies to `dst` from virtual address `src_va` in a given page table.
pub fn copy_in_bytes(
    pagetable: &UserPageTable,
    dst: &mut [u8],
    src_va: VirtAddr,
) -> Result<(), KernelError> {
    copy_in_raw(pagetable, dst.as_mut_ptr(), dst.len(), src_va)
}

/// Copies from user to kernel.
///
/// Copies to `dst` from virtual address `src_va` in a given page table.
pub fn copy_in_raw(
    pagetable: &UserPageTable,
    mut dst: *mut u8,
    mut dst_size: usize,
    mut src_va: VirtAddr,
) -> Result<(), KernelError> {
    while dst_size > 0 {
        let va0 = src_va.page_rounddown();
        let offset = src_va.addr() - va0.addr();
        let mut n = PAGE_SIZE - offset;
        if n > dst_size {
            n = dst_size;
        }
        let src_page = pagetable.fetch_page(va0, PtEntryFlags::UR)?;
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
    pagetable: &UserPageTable,
    mut dst: &mut [u8],
    mut src_va: VirtAddr,
) -> Result<(), KernelError> {
    while !dst.is_empty() {
        let va0 = src_va.page_rounddown();
        let src_page = pagetable.fetch_page(va0, PtEntryFlags::UR)?;

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
    Err(KernelError::Unknown)
}
