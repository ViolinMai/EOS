use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicUsize, Ordering};
use linked_list_allocator::LockedHeap;

pub const HEAP_SIZE: usize = 256 * 1024 * 1024; // 256 MB مساحة تشغيل ضخمة

#[global_allocator]
pub static HEAP_ALLOCATOR: DynamicKernelHeap = DynamicKernelHeap::new();

pub struct DynamicKernelHeap {
    inner: LockedHeap,
    allocated_bytes: AtomicUsize,
}

impl DynamicKernelHeap {
    pub const fn new() -> Self {
        Self {
            inner: LockedHeap::empty(),
            allocated_bytes: AtomicUsize::new(0),
        }
    }

    pub unsafe fn init(&self, start: *mut u8, size: usize) {
        unsafe {
            self.inner.lock().init(start, size);
        }
    }

    pub fn used(&self) -> usize {
        self.allocated_bytes.load(Ordering::Relaxed)
    }
}

unsafe impl GlobalAlloc for DynamicKernelHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { self.inner.alloc(layout) };
        if !ptr.is_null() {
            self.allocated_bytes.fetch_add(layout.size(), Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe {
            self.inner.dealloc(ptr, layout);
        }
        self.allocated_bytes.fetch_sub(layout.size(), Ordering::Relaxed);
    }
}
