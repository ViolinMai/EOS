use core::alloc::{GlobalAlloc, Layout};
use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicUsize, Ordering};
use core::mem::MaybeUninit;

pub const HEAP_SIZE: usize = 16 * 1024 * 1024; // 16 MB

#[allow(dead_code)]
#[repr(align(4096))]
pub struct HeapStorage(pub [u8; HEAP_SIZE]);

static mut HEAP_MEMORY: MaybeUninit<HeapStorage> = MaybeUninit::uninit();

pub struct LockedBumpAllocator {
    next: AtomicUsize,
}

impl LockedBumpAllocator {
    pub const fn new() -> Self {
        Self {
            next: AtomicUsize::new(0),
        }
    }

    pub fn used(&self) -> usize {
        self.next.load(Ordering::Relaxed)
    }
}

unsafe impl GlobalAlloc for LockedBumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let heap_start = addr_of_mut!(HEAP_MEMORY) as usize;
        let heap_end = heap_start + HEAP_SIZE;

        loop {
            let current_offset = self.next.load(Ordering::Relaxed);
            let current_addr = heap_start + current_offset;

            let align = layout.align();
            let alloc_start = (current_addr + align - 1) & !(align - 1);
            let new_offset = (alloc_start - heap_start) + layout.size();

            if heap_start + new_offset > heap_end {
                return core::ptr::null_mut();
            }

            if self
                .next
                .compare_exchange_weak(current_offset, new_offset, Ordering::SeqCst, Ordering::Relaxed)
                .is_ok()
            {
                return alloc_start as *mut u8;
            }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
pub static HEAP_ALLOCATOR: LockedBumpAllocator = LockedBumpAllocator::new();
