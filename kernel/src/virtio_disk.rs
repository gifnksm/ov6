use core::ptr;

use crate::bio::Buf;

mod ffi {
    use core::ffi::c_int;

    use crate::bio::Buf;

    unsafe extern "C" {
        pub fn virtio_disk_init();
        pub fn virtio_disk_intr();
        pub fn virtio_disk_rw(b: *mut Buf, write: c_int);
    }
}

pub fn init() {
    unsafe {
        ffi::virtio_disk_init();
    }
}

pub fn handle_interrupt() {
    unsafe {
        ffi::virtio_disk_intr();
    }
}

pub fn read(b: &mut Buf) {
    unsafe { ffi::virtio_disk_rw(ptr::from_mut(b), 0) }
}

pub fn write(b: &Buf) {
    unsafe {
        ffi::virtio_disk_rw(ptr::from_ref(b).cast_mut(), 1);
    }
}
