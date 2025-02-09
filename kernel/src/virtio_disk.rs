mod ffi {
    unsafe extern "C" {
        pub fn virtio_disk_init();
        pub fn virtio_disk_intr();
    }
}

pub fn init() {
    unsafe {
        ffi::virtio_disk_init();
    }
}

pub(crate) fn handle_interrupt() {
    unsafe {
        ffi::virtio_disk_intr();
    }
}
