#[cfg(target_arch = "riscv64")]
pub fn print_backtrace() {
    eprintln!("backtrace:");

    let mut fp: *const *const usize;
    unsafe {
        core::arch::asm!(
            "mv {fp}, s0",
            fp = out(reg) fp,
        );
    }

    let mut depth = 0;
    while !fp.is_null() {
        let ra = unsafe { *fp.sub(1) };
        if !ra.is_null() {
            eprintln!("{ra:#p}");
        }
        let prev_fp = unsafe { *fp.sub(2) };
        fp = prev_fp.cast();
        depth += 1;

        if depth > 100 {
            eprintln!("too long stack chain. abort printing");
            break;
        }
    }
}

#[cfg(not(target_arch = "riscv64"))]
pub fn print_backtrace() {
    todo!()
}
