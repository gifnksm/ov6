use core::{ffi::CStr, slice};

use ov6_types::path::Path;

use super::ProcPrivateData;
use crate::{
    error::KernelError,
    fs::{self, LockedTxInode},
    memory::{
        PAGE_SIZE, PageRound as _, VirtAddr, page_table::PtEntryFlags, user::UserPageTable, vm,
    },
    param::{MAX_ARG, USER_STACK},
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

pub fn exec(
    p: &Proc,
    private: &mut ProcPrivateData,
    path: &Path,
    argv: *const *const u8,
) -> Result<(usize, usize), KernelError> {
    let tx = fs::begin_tx();
    let mut ip = fs::path::resolve(&tx, private, path)?;
    let mut lip = ip.lock();

    // Check ELF header
    let mut elf = ElfHeader::zero();

    let nread = lip.read(
        private,
        false,
        VirtAddr::new((&raw mut elf).addr()),
        0,
        size_of::<ElfHeader>(),
    )?;
    if nread != size_of::<ElfHeader>() {
        return Err(KernelError::Unknown);
    }
    if elf.magic != ELF_MAGIC {
        return Err(KernelError::Unknown);
    }

    let mut pt = UserPageTable::new(private.trapframe().unwrap())?;

    // Load program into memory.
    if matches!(
        load_segments(private, &mut lip, &mut pt, &elf),
        Err(KernelError::Unknown)
    ) {
        return Err(KernelError::Unknown);
    }

    lip.unlock();
    ip.put();
    tx.end();

    if allocate_stack_pages(&mut pt).is_err() {
        return Err(KernelError::Unknown);
    }

    let sp = pt.size();
    let stack_base = sp - USER_STACK * PAGE_SIZE;

    // Push argument strings, prepare rest of stack in ustack.
    let Ok((sp, argc)) = push_arguments(&mut pt, sp, stack_base, argv) else {
        return Err(KernelError::Unknown);
    };

    let argv = sp;

    // Save program name for debugging.
    let name = path.file_name().unwrap();
    p.shared().lock().set_name(name);

    // Commit to the user image.
    private.update_pagetable(pt);
    private.trapframe_mut().unwrap().epc = elf.entry.try_into().unwrap(); // initial pogram counter = main
    private.trapframe_mut().unwrap().sp = sp; // initial stack pointer

    Ok((argc, argv))
}

fn load_segments<const READ_ONLY: bool>(
    private: &mut ProcPrivateData,
    lip: &mut LockedTxInode<READ_ONLY>,
    pagetable: &mut UserPageTable,
    elf: &ElfHeader,
) -> Result<(), KernelError> {
    for i in 0..elf.phnum {
        let off = usize::try_from(elf.phoff).unwrap() + usize::from(i) * size_of::<ProgramHeader>();
        let mut ph = ProgramHeader::zero();
        lip.read(
            private,
            false,
            VirtAddr::new((&raw mut ph).addr()),
            off,
            size_of::<ProgramHeader>(),
        )?;
        if ph.ty != ELF_PROG_LOAD {
            continue;
        }
        if ph.memsz < ph.filesz {
            return Err(KernelError::Unknown);
        }
        if ph.vaddr.checked_add(ph.memsz).is_none() {
            return Err(KernelError::Unknown);
        }
        if !usize::try_from(ph.vaddr).unwrap().is_page_aligned() {
            return Err(KernelError::Unknown);
        }
        pagetable.grow_to(
            usize::try_from(ph.vaddr + ph.memsz).unwrap(),
            flags2perm(ph.flags),
        )?;
        load_segment(
            private,
            pagetable,
            VirtAddr::new(ph.vaddr.try_into().unwrap()),
            lip,
            ph.off.try_into().unwrap(),
            ph.filesz.try_into().unwrap(),
        )?;
    }

    Ok(())
}

/// Loads a program segment into pagetable at virtual address `va`.
///
/// `va` must be page-aligned.
fn load_segment<const READ_ONLY: bool>(
    private: &mut ProcPrivateData,
    pagetable: &UserPageTable,
    va: VirtAddr,
    lip: &mut LockedTxInode<READ_ONLY>,
    offset: usize,
    sz: usize,
) -> Result<(), KernelError> {
    assert!(va.is_page_aligned());

    for i in (0..sz).step_by(PAGE_SIZE) {
        let pa = pagetable
            .resolve_virtual_address(va.byte_add(i), PtEntryFlags::U)
            .unwrap();

        let n = if sz - i < PAGE_SIZE {
            sz - i
        } else {
            PAGE_SIZE
        };

        let nread = lip.read(private, false, VirtAddr::new(pa.addr()), offset + i, n)?;
        if nread != n {
            return Err(KernelError::Unknown);
        }
    }

    Ok(())
}

/// Allocates some pages at the next page boundary.
///
/// Makes the first inaccessible as a stack guard.
/// Uses the rest as the user stack.
fn allocate_stack_pages(pagetable: &mut UserPageTable) -> Result<(), KernelError> {
    let size = pagetable.size().page_roundup();
    pagetable.grow_to(size + (USER_STACK + 1) * PAGE_SIZE, PtEntryFlags::W)?;
    pagetable.forbide_user_access(VirtAddr::new(
        pagetable.size() - (USER_STACK + 1) * PAGE_SIZE,
    ));
    Ok(())
}

fn push_arguments(
    pagetable: &mut UserPageTable,
    mut sp: usize,
    stack_base: usize,
    argv: *const *const u8,
) -> Result<(usize, usize), KernelError> {
    let mut ustack = [0_usize; MAX_ARG];

    let mut argc = 0;
    loop {
        let arg = unsafe { *argv.add(argc) };
        if arg.is_null() {
            break;
        }
        if argc >= MAX_ARG {
            return Err(KernelError::Unknown);
        }
        let arg = unsafe { CStr::from_ptr(arg) };
        sp -= arg.to_bytes_with_nul().len();
        sp -= sp % 16; // risc-v sp must be 16-byte aligned
        if sp < stack_base {
            return Err(KernelError::Unknown);
        }
        vm::copy_out_bytes(pagetable, VirtAddr::new(sp), arg.to_bytes_with_nul())?;
        ustack[argc] = sp;
        argc += 1;
    }
    ustack[argc] = 0;

    // push the array of argv[] pointers.
    sp -= (argc + 1) * size_of::<usize>();
    sp -= sp % 16;
    if sp < stack_base {
        return Err(KernelError::Unknown);
    }
    let src =
        unsafe { slice::from_raw_parts(ustack.as_ptr().cast(), (argc + 1) * size_of::<usize>()) };
    vm::copy_out_bytes(pagetable, VirtAddr::new(sp), src)?;
    Ok((sp, argc))
}
