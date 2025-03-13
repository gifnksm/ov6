use core::{mem, ptr, slice};

use dataview::{Pod, PodMethods as _};
use ov6_syscall::{UserMutRef, UserMutSlice, UserRef, UserSlice};

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
    mut dst: UserMutRef<T>,
    src: &T,
) -> Result<(), KernelError>
where
    T: Pod,
{
    let src = unsafe { slice::from_raw_parts(ptr::from_ref(src).cast(), mem::size_of::<T>()) };
    copy_out_bytes(pagetable, dst.as_bytes_mut(), src)
}

/// Copies from kernel to user.
///
/// Copies from `src` to virtual address `dst_va` in a given page table.
pub fn copy_out_bytes(
    pagetable: &mut UserPageTable,
    dst: UserMutSlice<u8>,
    mut src: &[u8],
) -> Result<(), KernelError> {
    assert_eq!(dst.len(), src.len());
    let mut dst_va = VirtAddr::new(dst.addr());
    while !src.is_empty() {
        let va0 = dst_va.page_rounddown();
        if va0 >= VirtAddr::MAX {
            return Err(KernelError::TooLargeVirtualAddress(dst_va));
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
pub fn copy_in<T>(pagetable: &UserPageTable, src: UserRef<T>) -> Result<T, KernelError>
where
    T: Pod,
{
    let mut dst = T::zeroed();
    copy_in_bytes(pagetable, dst.as_bytes_mut(), src.as_bytes())?;
    Ok(dst)
}

/// Copies from user to kernel.
///
/// Copies to `dst` from virtual address `src_va` in a given page table.
pub fn copy_in_bytes(
    pagetable: &UserPageTable,
    mut dst: &mut [u8],
    src: UserSlice<u8>,
) -> Result<(), KernelError> {
    assert_eq!(src.len(), dst.len());
    let mut src_va = VirtAddr::new(src.addr());
    while !dst.is_empty() {
        let va0 = src_va.page_rounddown();
        let offset = src_va.addr() - va0.addr();
        let mut n = PAGE_SIZE - offset;
        if n > dst.len() {
            n = dst.len();
        }
        let src_page = pagetable.fetch_page(va0, PtEntryFlags::UR)?;
        let src = &src_page[offset..][..n];
        dst[..n].copy_from_slice(src);
        dst = &mut dst[n..];
        src_va = va0.byte_add(PAGE_SIZE);
    }

    Ok(())
}
