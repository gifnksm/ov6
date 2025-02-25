use core::{alloc::GlobalAlloc, ptr};

use crate::{error::Error, os::xv6::syscall, sync::spin::Mutex};

/// Header for free block
///
/// Memory layout:
///
///     `| Header | Data | Header | Data | ...`
///
/// Each block is aligned to 16 bytes.
#[repr(align(16))]
struct Header {
    /// Next free block
    next: *mut Header,
    /// Size in units of `Header`
    size: usize,
}

unsafe impl Sync for Header {}

static mut BASE: Header = Header {
    next: &raw mut BASE,
    size: 0,
};

// Circular list of free blocks
static FREE_LIST: Mutex<*mut Header> = Mutex::new(&raw mut BASE);

fn malloc(nbytes: usize, free_list: &mut *mut Header) -> Result<*mut u8, Error> {
    let nunits = nbytes.div_ceil(size_of::<Header>()) + 1;

    unsafe {
        let mut prevp = *free_list;
        let mut p = (*prevp).next;
        loop {
            if (*p).size >= nunits {
                // big enough
                if (*p).size == nunits {
                    // exactly
                    (*prevp).next = (*p).next;
                } else {
                    // allocate tail end
                    (*p).size -= nunits;
                    p = p.add((*p).size);
                    (*p).size = nunits;
                }
                *free_list = prevp;
                return Ok(p.add(1).cast());
            }

            if p == *free_list {
                // wrapped around free list
                p = expand_heap(nunits, free_list)?;
            }

            prevp = p;
            p = (*p).next;
        }
    }
}

unsafe fn free(ap: *mut u8, free_list: &mut *mut Header) {
    unsafe {
        // point to block header
        let bp = ap.cast::<Header>().sub(1);

        let mut p = *free_list;
        while !(bp > p && bp < (*p).next) {
            if p >= (*p).next && (bp > p || bp < (*p).next) {
                // freed block at start or end of arena
                break;
            }
            p = (*p).next;
        }

        if bp.add((*bp).size) == (*p).next {
            // join to upper neighbor
            (*bp).size += (*(*p).next).size;
            (*bp).next = (*(*p).next).next;
        } else {
            (*bp).next = (*p).next;
        }

        if p.add((*p).size) == bp {
            // join to lower neighbor
            (*p).size += (*bp).size;
            (*p).next = (*bp).next;
        } else {
            (*p).next = bp;
        }

        *free_list = p;
    }
}

fn expand_heap(mut nunits: usize, free_list: &mut *mut Header) -> Result<*mut Header, Error> {
    unsafe {
        if nunits < 4096 {
            nunits = 4096;
        }

        let p = syscall::sbrk(nunits * size_of::<Header>())?;

        let hp = p.cast::<Header>();
        (*hp).size = nunits;
        free(hp.add(1) as _, free_list);

        Ok(*free_list)
    }
}

#[global_allocator]
static GLOBAL: Allocator = Allocator;

struct Allocator;

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        assert_eq!(align_of::<Header>() % layout.align(), 0);
        let mut free_list = FREE_LIST.lock();
        malloc(layout.size(), &mut free_list).unwrap_or(ptr::null_mut())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: core::alloc::Layout) {
        let mut free_list = FREE_LIST.lock();
        unsafe { free(ptr, &mut free_list) }
    }
}
