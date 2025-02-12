mod ffi {
    use core::ffi::c_int;

    unsafe extern "C" {
        pub type Pipe;
        pub fn pipeclose(pi: *mut Pipe, writable: c_int);
        pub fn piperead(pi: *mut Pipe, addr: u64, n: c_int) -> c_int;
        pub fn pipewrite(pi: &mut Pipe, addr: u64, n: c_int) -> c_int;
    }
}

pub use ffi::Pipe;

use crate::vm::VirtAddr;

pub fn close(pipe: &mut Pipe, writable: bool) {
    unsafe { ffi::pipeclose(pipe, writable.into()) }
}

pub fn read(pipe: &mut Pipe, addr: VirtAddr, n: usize) -> Result<usize, ()> {
    let sz = unsafe { ffi::piperead(pipe, addr.addr() as u64, n as i32) };
    if sz < 0 {
        return Err(());
    }
    Ok(sz as usize)
}

pub fn write(pipe: &mut Pipe, addr: VirtAddr, n: usize) -> Result<usize, ()> {
    let sz = unsafe { ffi::pipewrite(pipe, addr.addr() as u64, n as i32) };
    if sz < 0 {
        return Err(());
    }
    Ok(sz as usize)
}
