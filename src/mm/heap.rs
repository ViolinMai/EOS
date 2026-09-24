use linked_list_allocator::LockedHeap as RawLockedHeap;
use core::alloc::{GlobalAlloc, Layout};

pub const HEAP_SIZE: usize = 256 * 1024 * 1024; // 256 MB Kernel Heap

pub struct LockedHeap(RawLockedHeap);

impl LockedHeap {
    pub const fn empty() -> Self {
        Self(RawLockedHeap::empty())
    }

    pub unsafe fn init(&self, start: *mut u8, size: usize) {
        unsafe {
            self.0.lock().init(start, size);
        }
    }

    pub fn used(&self) -> usize {
        self.0.lock().used()
    }
}

unsafe impl GlobalAlloc for LockedHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { self.0.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { self.0.dealloc(ptr, layout) }
    }
}

#[global_allocator]
pub static HEAP_ALLOCATOR: LockedHeap = LockedHeap::empty();
