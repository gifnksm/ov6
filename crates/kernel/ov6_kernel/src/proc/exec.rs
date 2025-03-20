use core::slice;

use dataview::PodMethods as _;
use ov6_syscall::{UserMutSlice, UserSlice};
use ov6_types::path::Path;
use safe_cast::{SafeFrom as _, SafeInto as _};

use super::ProcPrivateData;
use crate::{
    error::KernelError,
    fs::{self, LockedTxInode},
    memory::{
        PAGE_SIZE, PageRound as _, VirtAddr,
        addr::{Validate as _, Validated},
        page_table::PtEntryFlags,
        vm_user::UserPageTable,
    },
    param::USER_STACK,
    proc::{
        Proc,
        elf::{ELF_MAGIC, ELF_PROG_LOAD, ElfHeader, ProgramHeader},
    },
};

fn flags2perm(flags: u32) -> PtEntryFlags {
    let mut perm = PtEntryFlags::empty();
    if flags & 0x1 != 0 {
        perm.insert(PtEntryFlags::X);
    }
    if flags & 0x2 != 0 {
        perm.insert(PtEntryFlags::W);
    }
    perm
}

fn arg_stack_size(arg_data_size: usize, arg_len: usize) -> Option<usize> {
    let argv_size = arg_len.checked_add(1)?.checked_mul(size_of::<usize>())?;
    let stack_size = argv_size.checked_add(arg_data_size)?;
    Some(stack_size.next_multiple_of(16))
}

pub fn exec(
    p: &Proc,
    private: &mut ProcPrivateData,
    path: &Path,
    argv: &Validated<UserSlice<Validated<UserSlice<u8>>>>,
    arg_data_size: usize,
) -> Result<(usize, usize), KernelError> {
    let user_stack_size = USER_STACK * PAGE_SIZE;
    let arg_stack_size = arg_stack_size(arg_data_size, argv.len())
        .filter(|size| *size <= user_stack_size)
        .ok_or(KernelError::ArgumentListTooLarge)?;

    let tx = fs::begin_tx()?;
    let cwd = private.cwd().clone().into_tx(&tx);
    let mut ip = fs::path::resolve(&tx, cwd, path)?;
    let mut lip = ip.force_wait_lock();

    // Check ELF header
    let mut elf = ElfHeader::zero();

    let nread = lip.read(elf.as_bytes_mut().into(), 0)?;
    if nread != size_of::<ElfHeader>() {
        return Err(KernelError::InvalidExecutable);
    }
    if elf.magic != ELF_MAGIC {
        return Err(KernelError::InvalidExecutable);
    }

    let mut pt = UserPageTable::new(private.trapframe())?;

    // Load program into memory.
    load_segments(&mut lip, &mut pt, &elf)?;

    lip.unlock();
    ip.put();
    tx.end();

    allocate_stack_pages(&mut pt)?;

    let sp = pt.size();

    // Push argument strings, prepare rest of stack in ustack.
    let (sp, argc) = push_arguments(&mut pt, private.pagetable(), sp, arg_stack_size, argv);

    let argv = sp;

    // Save program name for debugging.
    let name = path.file_name().unwrap();
    p.shared().lock().set_name(name);

    // Commit to the user image.
    private.update_pagetable(pt);
    private.trapframe_mut().epc = elf.entry.safe_into(); // initial pogram counter = main
    private.trapframe_mut().sp = sp; // initial stack pointer

    Ok((argc, argv))
}

fn load_segments<const READ_ONLY: bool>(
    lip: &mut LockedTxInode<READ_ONLY>,
    new_pt: &mut UserPageTable,
    elf: &ElfHeader,
) -> Result<(), KernelError> {
    for i in 0..elf.phnum {
        let off = usize::safe_from(elf.phoff) + usize::from(i) * size_of::<ProgramHeader>();
        let mut ph = ProgramHeader::zero();
        lip.read(ph.as_bytes_mut().into(), off)?;
        if ph.ty != ELF_PROG_LOAD {
            continue;
        }
        if ph.memsz < ph.filesz {
            return Err(KernelError::InvalidExecutable);
        }
        if ph.vaddr.checked_add(ph.memsz).is_none() {
            return Err(KernelError::InvalidExecutable);
        }

        let va_start = VirtAddr::new(ph.vaddr.safe_into())?;
        if !va_start.is_page_aligned() {
            return Err(KernelError::InvalidExecutable);
        }
        let va_end = va_start.byte_add(ph.memsz.safe_into())?;

        new_pt.grow_to(va_end.addr(), flags2perm(ph.flags))?;

        load_segment(
            new_pt,
            va_start,
            lip,
            ph.off.safe_into(),
            ph.filesz.safe_into(),
        )?;
    }

    Ok(())
}

/// Loads a program segment into pagetable at virtual address `va`.
///
/// `va` must be page-aligned.
fn load_segment<const READ_ONLY: bool>(
    new_pt: &UserPageTable,
    va: VirtAddr,
    lip: &mut LockedTxInode<READ_ONLY>,
    file_offset: usize,
    file_size: usize,
) -> Result<(), KernelError> {
    assert!(va.is_page_aligned());

    for i in (0..file_size).step_by(PAGE_SIZE) {
        let va = va.byte_add(i).unwrap();
        let pa = new_pt.resolve_virtual_address(va, PtEntryFlags::U).unwrap();

        let n = if file_size - i < PAGE_SIZE {
            file_size - i
        } else {
            PAGE_SIZE
        };

        let dst = unsafe { slice::from_raw_parts_mut(pa.as_mut_ptr().as_ptr(), n) };
        let nread = lip.read(dst.into(), file_offset + i)?;
        if nread != n {
            return Err(KernelError::InvalidExecutable);
        }
    }

    Ok(())
}

/// Allocates some pages at the next page boundary.
///
/// Makes the first inaccessible as a stack guard.
/// Uses the rest as the user stack.
fn allocate_stack_pages(pt: &mut UserPageTable) -> Result<(), KernelError> {
    let size = pt.size().page_roundup();
    pt.grow_to(size + (USER_STACK + 1) * PAGE_SIZE, PtEntryFlags::W)?;
    pt.forbide_user_access(
        VirtAddr::new(pt.size() - (USER_STACK + 1) * PAGE_SIZE)?.virt_page_num(),
    )?;
    Ok(())
}

fn push_arguments(
    dst_pt: &mut UserPageTable,
    src_pt: &UserPageTable,
    sp: usize,
    arg_stack_size: usize,
    argv: &Validated<UserSlice<Validated<UserSlice<u8>>>>,
) -> (usize, usize) {
    let arg_top = sp - arg_stack_size;
    assert_eq!(arg_top % 16, 0);

    let mut arg_stack = UserMutSlice::from_raw_parts(arg_top, arg_stack_size)
        .validate(dst_pt)
        .unwrap();
    let argv_size = (argv.len() + 1) * size_of::<usize>();
    let mut dst_argv = arg_stack.take_mut(argv_size).cast_mut::<usize>();
    let mut dst_chars = arg_stack.skip_mut(argv_size);

    for i in 0..argv.len() {
        dst_pt.copy_k2u(&mut dst_argv.nth_mut(i), &dst_chars.addr());

        let uarg = src_pt.copy_u2k(&argv.nth(i));
        UserPageTable::copy_u2u_bytes(dst_pt, &mut dst_chars.take_mut(uarg.len()), src_pt, &uarg);
        dst_chars = dst_chars.skip_mut(uarg.len());
        dst_pt.copy_k2u(&mut dst_chars.nth_mut(0), &0);
        dst_chars = dst_chars.skip_mut(1);
    }
    dst_pt.copy_k2u(&mut dst_argv.nth_mut(argv.len()), &0);

    (arg_top, argv.len())
}
