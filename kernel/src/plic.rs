mod ffi {
    use core::ffi::c_int;

    unsafe extern "C" {
        pub fn plicinit();
        pub fn plicinithart();
        pub fn plic_claim() -> c_int;
        pub fn plic_complete(irq: c_int);
    }
}

pub fn init() {
    unsafe { ffi::plicinit() }
}

pub fn init_hart() {
    unsafe { ffi::plicinithart() }
}

pub fn claim() -> usize {
    unsafe { ffi::plic_claim() as usize }
}

pub fn complete(irq: usize) {
    unsafe { ffi::plic_complete(irq as i32) }
}
