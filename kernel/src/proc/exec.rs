use core::{ffi::CStr, slice};

use crate::{
    error::Error,
    fs::{self, LockedTxInode},
    memory::vm::{self, PAGE_SIZE, PageRound as _, PageTable, PtEntryFlags, VirtAddr},
    param::{MAX_ARG, USER_STACK},
    proc::{
        self, Proc,
        elf::{ELF_MAGIC, ELF_PROG_LOAD, ElfHeader, ProgramHeader},
    },
};

use super::ProcPrivateData;

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
    path: &[u8],
    argv: *const *const u8,
) -> Result<usize, Error> {
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
        return Err(Error::Unknown);
    }
    if elf.magic != ELF_MAGIC {
        return Err(Error::Unknown);
    }

    let mut pagetable = proc::create_pagetable(private).ok_or(Error::Unknown)?;

    // Load program into memory.
    let mut sz = 0;
    if let Err(Error::Unknown) = load_segments(private, &mut lip, pagetable.as_mut(), &mut sz, &elf)
    {
        proc::free_pagetable(pagetable, sz);
        return Err(Error::Unknown);
    }

    lip.unlock();
    ip.put();
    tx.end();

    if allocate_stack_pages(&mut pagetable, &mut sz).is_err() {
        proc::free_pagetable(pagetable, sz);
        return Err(Error::Unknown);
    }

    let sp = sz;
    let stack_base = sp - USER_STACK * PAGE_SIZE;

    // Push argument strings, prepare rest of stack in ustack.
    let Ok((sp, argc)) = push_arguments(&pagetable, sp, stack_base, argv) else {
        proc::free_pagetable(pagetable, sz);
        return Err(Error::Unknown);
    };

    // arguments to user main(argc, argv).
    // argc is returned via the system call return
    // value, which goes in a0.
    private.trapframe_mut().unwrap().a1 = sp;

    // Save program name for debugging.
    let name = path
        .iter()
        .position(|&c| c == b'/')
        .map(|i| &path[i..])
        .unwrap_or(path);
    p.shared().lock().set_name(name);

    // Commit to the user image.
    private.update_pagetable(pagetable, sz);
    private.trapframe_mut().unwrap().epc = elf.entry as usize; // initial pogram counter = main
    private.trapframe_mut().unwrap().sp = sp; // initial stack pointer

    Ok(argc)
}

fn load_segments<const READ_ONLY: bool>(
    private: &ProcPrivateData,
    lip: &mut LockedTxInode<READ_ONLY>,
    pagetable: &mut PageTable,
    sz: &mut usize,
    elf: &ElfHeader,
) -> Result<(), Error> {
    for i in 0..elf.phnum {
        let off = elf.phoff as usize + usize::from(i) * size_of::<ProgramHeader>();
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
            return Err(Error::Unknown);
        }
        if ph.vaddr.checked_add(ph.memsz).is_none() {
            return Err(Error::Unknown);
        }
        if !(ph.vaddr as usize).is_page_aligned() {
            return Err(Error::Unknown);
        }
        *sz = vm::user::alloc(
            pagetable,
            *sz,
            (ph.vaddr + ph.memsz) as usize,
            flags2perm(ph.flags),
        )?;
        load_segment(
            private,
            pagetable,
            VirtAddr::new(ph.vaddr as usize),
            lip,
            ph.off as usize,
            ph.filesz as usize,
        )?;
    }

    Ok(())
}

/// Loads a program segment into pagetable at virtual address `va`.
///
/// `va` must be page-aligned.
fn load_segment<const READ_ONLY: bool>(
    private: &ProcPrivateData,
    pagetable: &PageTable,
    va: VirtAddr,
    lip: &mut LockedTxInode<READ_ONLY>,
    offset: usize,
    sz: usize,
) -> Result<(), Error> {
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
            return Err(Error::Unknown);
        }
    }

    Ok(())
}

/// Allocates some pages at the next page boundary.
///
/// Makes the first inaccessible as a stack guard.
/// Uses the rest as the user stack.
fn allocate_stack_pages(pagetable: &mut PageTable, sz: &mut usize) -> Result<(), Error> {
    *sz = sz.page_roundup();
    *sz = vm::user::alloc(
        pagetable,
        *sz,
        *sz + (USER_STACK + 1) * PAGE_SIZE,
        PtEntryFlags::W,
    )?;
    vm::user::forbide_user_access(pagetable, VirtAddr::new(*sz - (USER_STACK + 1) * PAGE_SIZE));
    Ok(())
}

fn push_arguments(
    pagetable: &PageTable,
    mut sp: usize,
    stack_base: usize,
    argv: *const *const u8,
) -> Result<(usize, usize), Error> {
    let mut ustack = [0usize; MAX_ARG];

    let mut argc = 0;
    loop {
        let arg = unsafe { *argv.add(argc) };
        if arg.is_null() {
            break;
        }
        if argc >= MAX_ARG {
            return Err(Error::Unknown);
        }
        let arg = unsafe { CStr::from_ptr(arg) };
        sp -= arg.to_bytes_with_nul().len();
        sp -= sp % 16; // risc-v sp must be 16-byte aligned
        if sp < stack_base {
            return Err(Error::Unknown);
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
        return Err(Error::Unknown);
    }
    let src =
        unsafe { slice::from_raw_parts(ustack.as_ptr().cast(), (argc + 1) * size_of::<usize>()) };
    vm::copy_out_bytes(pagetable, VirtAddr::new(sp), src)?;
    Ok((sp, argc))
}
