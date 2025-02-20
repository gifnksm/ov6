use core::{ffi::CStr, ptr::NonNull, slice};

use crate::{
    fs::{
        self, Inode,
        log::{self, Tx},
    },
    memory::vm::{self, PAGE_SIZE, PageRound as _, PageTable, PtEntryFlags, VirtAddr},
    param::{MAX_ARG, USER_STACK},
    proc::{
        self, Proc,
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

pub fn exec(path: &[u8], argv: *const *const u8) -> Result<usize, ()> {
    let p = Proc::current();
    let tx = log::begin_tx();
    let ip = fs::resolve_path(&tx, p, path)?;

    let (elf, mut pagetable, mut sz) = fs::inode_with_lock(&tx, ip, |ip| {
        // Check ELF header
        let mut elf = ElfHeader::zero();

        if fs::read_inode(
            &tx,
            p,
            ip,
            false,
            VirtAddr::new((&raw mut elf).addr()),
            0,
            size_of::<ElfHeader>(),
        ) != Ok(size_of::<ElfHeader>())
        {
            return Err(());
        }
        if elf.magic != ELF_MAGIC {
            return Err(());
        }

        let mut pagetable = proc::create_pagetable(p).ok_or(())?;

        // Load program into memory.
        let mut sz = 0;
        if let Err(()) = load_segments(&tx, p, ip, unsafe { pagetable.as_mut() }, &mut sz, &elf) {
            proc::free_pagetable(pagetable, sz);
            return Err(());
        }

        Ok((elf, pagetable, sz))
    })?;
    tx.end();

    if allocate_stack_pages(unsafe { pagetable.as_mut() }, &mut sz).is_err() {
        proc::free_pagetable(pagetable, sz);
        return Err(());
    }

    let sp = sz;
    let stack_base = sp - USER_STACK * PAGE_SIZE;

    // Push argument strings, prepare rest of stack in ustack.
    let Ok((sp, argc)) = push_arguments(unsafe { pagetable.as_ref() }, sp, stack_base, argv) else {
        proc::free_pagetable(pagetable, sz);
        return Err(());
    };

    // arguments to user main(argc, argv).
    // argc is returned via the system call return
    // value, which goes in a0.
    p.trapframe_mut().unwrap().a1 = sp as u64;

    // Save program name for debugging.
    let name = path
        .iter()
        .position(|&c| c == b'/')
        .map(|i| &path[i..])
        .unwrap_or(path);
    let dst = unsafe { p.name_mut().as_mut() };
    dst.fill(0);
    let copy_len = usize::min(dst.len() - 1, name.len());
    dst[..copy_len].copy_from_slice(&name[..copy_len]);

    // Commit to the user image.
    p.update_pagetable(pagetable, sz);
    p.trapframe_mut().unwrap().epc = elf.entry; // initial pogram counter = main
    p.trapframe_mut().unwrap().sp = sp as u64; // initial stack pointer

    Ok(argc)
}

fn load_segments<const READ_ONLY: bool>(
    tx: &Tx<READ_ONLY>,
    p: &Proc,
    ip: NonNull<Inode>,
    pagetable: &mut PageTable,
    sz: &mut usize,
    elf: &ElfHeader,
) -> Result<(), ()> {
    for i in 0..elf.phnum {
        let off = elf.phoff as usize + usize::from(i) * size_of::<ProgramHeader>();
        let mut ph = ProgramHeader::zero();
        fs::read_inode(
            tx,
            p,
            ip,
            false,
            VirtAddr::new((&raw mut ph).addr()),
            off,
            size_of::<ProgramHeader>(),
        )?;
        if ph.ty != ELF_PROG_LOAD {
            continue;
        }
        if ph.memsz < ph.filesz {
            return Err(());
        }
        if ph.vaddr.checked_add(ph.memsz).is_none() {
            return Err(());
        }
        if !(ph.vaddr as usize).is_page_aligned() {
            return Err(());
        }
        *sz = vm::user::alloc(
            pagetable,
            *sz,
            (ph.vaddr + ph.memsz) as usize,
            flags2perm(ph.flags),
        )?;
        load_segment(
            tx,
            p,
            pagetable,
            VirtAddr::new(ph.vaddr as usize),
            ip,
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
    tx: &Tx<READ_ONLY>,
    p: &Proc,
    pagetable: &PageTable,
    va: VirtAddr,
    ip: NonNull<Inode>,
    offset: usize,
    sz: usize,
) -> Result<(), ()> {
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

        if fs::read_inode(tx, p, ip, false, VirtAddr::new(pa.addr()), offset + i, n) != Ok(n) {
            return Err(());
        }
    }

    Ok(())
}

/// Allocates some pages at the next page boundary.
///
/// Makes the first inaccessible as a stack guard.
/// Uses the rest as the user stack.
fn allocate_stack_pages(pagetable: &mut PageTable, sz: &mut usize) -> Result<(), ()> {
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
) -> Result<(usize, usize), ()> {
    let mut ustack = [0usize; MAX_ARG];

    let mut argc = 0;
    loop {
        let arg = unsafe { *argv.add(argc) };
        if arg.is_null() {
            break;
        }
        if argc >= MAX_ARG {
            return Err(());
        }
        let arg = unsafe { CStr::from_ptr(arg) };
        sp -= arg.to_bytes_with_nul().len();
        sp -= sp % 16; // risc-v sp must be 16-byte aligned
        if sp < stack_base {
            return Err(());
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
        return Err(());
    }
    let src =
        unsafe { slice::from_raw_parts(ustack.as_ptr().cast(), (argc + 1) * size_of::<usize>()) };
    vm::copy_out_bytes(pagetable, VirtAddr::new(sp), src)?;
    Ok((sp, argc))
}
