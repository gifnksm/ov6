use dataview::PodMethods as _;
use ov6_syscall::UserMutSlice;
use ov6_types::path::Path;
use safe_cast::{SafeFrom as _, SafeInto as _};

use super::ProcPrivateData;
use crate::{
    error::KernelError,
    fs::{self, LockedTxInode},
    memory::{
        PAGE_SIZE, PageRound as _, VirtAddr,
        addr::{AsGenericSliceOfSlice, GenericSliceOfSlice, Validate as _},
        page_table::{MapTarget, PtEntryFlags},
        vm_user::UserPageTable,
    },
    param::USER_STACK_PAGES,
    proc::{
        Proc,
        elf::{ELF_MAGIC, ELF_PROG_LOAD, ElfHeader, ProgramHeader},
    },
};

const PF_X: u32 = 0x1;
const PF_W: u32 = 0x2;
const PF_R: u32 = 0x4;

fn flags2perm(flags: u32) -> PtEntryFlags {
    let mut perm = PtEntryFlags::empty();
    if flags & PF_X != 0 {
        perm.insert(PtEntryFlags::X);
    }
    if flags & PF_W != 0 {
        perm.insert(PtEntryFlags::W);
    }
    if flags & PF_R != 0 {
        perm.insert(PtEntryFlags::R);
    }
    perm
}

fn arg_stack_size(arg_data_size: usize, arg_len: usize) -> Option<usize> {
    let argv_size = arg_len.checked_add(1)?.checked_mul(size_of::<usize>())?;
    let stack_size = argv_size.checked_add(arg_data_size)?;
    Some(stack_size.next_multiple_of(16))
}

pub fn exec<'a, A>(
    p: &Proc,
    private: &mut ProcPrivateData,
    path: &Path,
    argv: &'a A,
    arg_data_size: usize,
) -> Result<(usize, VirtAddr), KernelError>
where
    A: AsGenericSliceOfSlice<u8> + 'a,
{
    let user_stack_size = USER_STACK_PAGES * PAGE_SIZE;
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

    let mut pt = UserPageTable::new(private.pid)?;

    // Load program into memory.
    let segment_end = load_segments(&mut lip, &mut pt, &elf)?;
    assert!(segment_end.is_page_aligned());
    let heap_start = segment_end.byte_add(PAGE_SIZE)?.level_page_roundup(1);

    pt.set_heap_start(heap_start);

    lip.unlock();
    ip.put();
    tx.end();

    pt.alloc_stack()?;

    let sp = pt.stack_top();

    // Push argument strings, prepare rest of stack in ustack.
    let argv = argv.as_generic_slice_of_slice(private.pagetable());
    let (sp, argc) = push_arguments(&mut pt, sp, arg_stack_size, &argv);

    let argv = sp;

    // Save program name for debugging.
    let name = path.file_name().unwrap();
    p.shared().lock().set_name(name);

    // Commit to the user image.
    private.update_pagetable(pt);
    let tf = private.trapframe_mut();
    tf.epc = elf.entry.safe_into(); // initial pogram counter = main
    tf.user_registers.sp = sp.addr(); // initial stack pointer

    Ok((argc, argv))
}

fn load_segments<const READ_ONLY: bool>(
    lip: &mut LockedTxInode<READ_ONLY>,
    new_pt: &mut UserPageTable,
    elf: &ElfHeader,
) -> Result<VirtAddr, KernelError> {
    let mut segment_end = new_pt.heap_start();

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
        let va_end = va_start.byte_add(ph.memsz.safe_into())?;
        let perm = PtEntryFlags::U | flags2perm(ph.flags);

        let map_start = va_start.page_rounddown();
        let map_end = va_end.page_roundup();
        let map_size = map_end.checked_sub(map_start).unwrap();
        unsafe {
            new_pt.map_addrs(
                map_start,
                MapTarget::allocated_addr(true, false),
                map_size,
                perm,
            )?;
        }

        new_pt.validate(va_start..va_end, perm)?;

        load_segment(
            new_pt,
            va_start,
            lip,
            ph.off.safe_into(),
            ph.filesz.safe_into(),
        )?;

        segment_end = VirtAddr::max(segment_end, map_end);
    }

    Ok(segment_end)
}

/// Loads a program segment into pagetable at virtual address `va`.
///
/// `va` must be page-aligned.
fn load_segment<const READ_ONLY: bool>(
    new_pt: &mut UserPageTable,
    va: VirtAddr,
    lip: &mut LockedTxInode<READ_ONLY>,
    file_offset: usize,
    file_size: usize,
) -> Result<(), KernelError> {
    let mut va_start = va;
    let va_end = va.byte_add(file_size).unwrap();
    let mut copied = 0;
    while copied < file_size {
        let rest_len = file_size - copied;
        let mut dst_chunk = new_pt.fetch_chunk_mut(va_start, PtEntryFlags::U).unwrap();
        if dst_chunk.len() > rest_len {
            dst_chunk = &mut dst_chunk[..rest_len];
        }
        let nread = lip.read(dst_chunk.into(), file_offset + copied)?;
        if nread != dst_chunk.len() {
            return Err(KernelError::InvalidExecutable);
        }
        va_start = va_start.byte_add(dst_chunk.len()).unwrap();
        copied += dst_chunk.len();
    }
    assert_eq!(va_start, va_end);

    Ok(())
}

fn push_arguments(
    dst_pt: &mut UserPageTable,
    sp: VirtAddr,
    arg_stack_size: usize,
    argv: &GenericSliceOfSlice<u8>,
) -> (VirtAddr, usize) {
    let arg_top = sp.byte_sub(arg_stack_size).unwrap();
    assert_eq!(arg_top.addr() % 16, 0);

    let mut arg_stack = unsafe { UserMutSlice::from_raw_parts(arg_top.addr(), arg_stack_size) }
        .validate(dst_pt)
        .unwrap();
    let argv_size = (argv.len() + 1) * size_of::<usize>();
    let mut dst_argv = arg_stack.take_mut(argv_size).cast_mut::<usize>();
    let mut dst_chars = arg_stack.skip_mut(argv_size);

    for i in 0..argv.len() {
        dst_pt.copy_k2u(&mut dst_argv.nth_mut(i), &dst_chars.addr());

        let uarg = argv.nth(i);
        dst_pt.copy_x2u_bytes(&mut dst_chars.take_mut(uarg.len()), &uarg);
        dst_chars = dst_chars.skip_mut(uarg.len());
        dst_pt.copy_k2u(&mut dst_chars.nth_mut(0), &0);
        dst_chars = dst_chars.skip_mut(1);
    }
    dst_pt.copy_k2u(&mut dst_argv.nth_mut(argv.len()), &0);

    (arg_top, argv.len())
}
