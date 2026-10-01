use crate::log_info;
use core::sync::atomic::{AtomicBool, Ordering};

pub const PAGE_SIZE: usize = 4096;

pub struct FrameSpinlock { locked: AtomicBool, }
impl FrameSpinlock {
    pub const fn new() -> Self { Self { locked: AtomicBool::new(false) } }
    pub fn lock(&self) { while self.locked.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() { core::hint::spin_loop(); } }
    pub fn unlock(&self) { self.locked.store(false, Ordering::Release); }
}

pub struct BitmapFrameAllocator {
    bitmap: *mut u8,
    total_frames: usize,
    used_frames: usize,
    last_alloc_byte: usize,
    pub hhdm_offset: u64,
}

unsafe impl Send for BitmapFrameAllocator {}
unsafe impl Sync for BitmapFrameAllocator {}

static mut ALLOCATOR_INSTANCE: Option<BitmapFrameAllocator> = None;
pub static ALLOCATOR_LOCK: FrameSpinlock = FrameSpinlock::new();

#[inline]
fn is_entry_usable(entry: &limine::memmap::Entry) -> bool {
    let raw_val: u64 = unsafe { core::ptr::read_unaligned(&entry.type_ as *const _ as *const u64) };
    raw_val == 0
}

impl BitmapFrameAllocator {
    pub unsafe fn init(entries: &[&limine::memmap::Entry], hhdm_offset: u64) {
        let mut highest_address: u64 = 0;
        let mut usable_memory: usize = 0;
        for &entry in entries {
            let top = entry.base + entry.length;
            if top > highest_address { highest_address = top; }
            if is_entry_usable(entry) { usable_memory += entry.length as usize; }
        }
        let total_frames = (highest_address as usize + PAGE_SIZE - 1) / PAGE_SIZE;
        let bitmap_size = (total_frames + 7) / 8;
        let mut bitmap_phys_addr: Option<u64> = None;
        for &entry in entries {
            if is_entry_usable(entry) && entry.length as usize >= bitmap_size {
                bitmap_phys_addr = Some(entry.base);
                break;
            }
        }
        let bitmap_phys = bitmap_phys_addr.expect("Failed to find usable memory for Bitmap");
        let bitmap_virt = (bitmap_phys + hhdm_offset) as *mut u8;
        unsafe { core::ptr::write_bytes(bitmap_virt, 0xFF, bitmap_size); }

        let mut allocator = Self {
            bitmap: bitmap_virt, total_frames, used_frames: total_frames, last_alloc_byte: 0, hhdm_offset,
        };

        for &entry in entries {
            if is_entry_usable(entry) {
                let start_frame = (entry.base as usize) / PAGE_SIZE;
                let frame_count = (entry.length as usize) / PAGE_SIZE;
                for i in 0..frame_count { allocator.free_frame_index(start_frame + i); }
            }
        }
        let bitmap_start_frame = (bitmap_phys as usize) / PAGE_SIZE;
        let bitmap_frame_count = (bitmap_size + PAGE_SIZE - 1) / PAGE_SIZE;
        for i in 0..bitmap_frame_count { allocator.mark_frame_used(bitmap_start_frame + i); }

        log_info!("PMM", "Physical Memory: {} MB configured", usable_memory / (1024 * 1024));
        
        ALLOCATOR_LOCK.lock();
        unsafe { *core::ptr::addr_of_mut!(ALLOCATOR_INSTANCE) = Some(allocator); }
        ALLOCATOR_LOCK.unlock();
    }

    #[inline]
    fn mark_frame_used(&mut self, frame_index: usize) {
        if frame_index >= self.total_frames { return; }
        let byte_idx = frame_index / 8;
        let bit_idx = frame_index % 8;
        unsafe {
            let byte = self.bitmap.add(byte_idx);
            if (*byte & (1 << bit_idx)) == 0 { *byte |= 1 << bit_idx; self.used_frames += 1; }
        }
    }

    #[inline]
    pub fn free_frame_index(&mut self, frame_index: usize) {
        if frame_index >= self.total_frames { return; }
        let byte_idx = frame_index / 8;
        let bit_idx = frame_index % 8;
        unsafe {
            let byte = self.bitmap.add(byte_idx);
            if (*byte & (1 << bit_idx)) != 0 { *byte &= !(1 << bit_idx); self.used_frames -= 1; }
        }
        if byte_idx < self.last_alloc_byte { self.last_alloc_byte = byte_idx; }
    }

    pub fn allocate_frame(&mut self) -> Option<u64> {
        let start_byte = self.last_alloc_byte;
        let total_bytes = self.total_frames / 8;
        for i in 0..total_bytes {
            let byte_idx = (start_byte + i) % total_bytes;
            unsafe {
                let byte = self.bitmap.add(byte_idx);
                if *byte != 0xFF {
                    for bit_idx in 0..8 {
                        if (*byte & (1 << bit_idx)) == 0 {
                            let frame_idx = byte_idx * 8 + bit_idx;
                            self.mark_frame_used(frame_idx);
                            self.last_alloc_byte = byte_idx;
                            return Some((frame_idx * PAGE_SIZE) as u64);
                        }
                    }
                }
            }
        }
        None
    }
}

pub fn allocate_frame_safe() -> Option<u64> {
    ALLOCATOR_LOCK.lock();
    let res = unsafe {
        if let Some(alloc) = (*core::ptr::addr_of_mut!(ALLOCATOR_INSTANCE)).as_mut() {
            alloc.allocate_frame()
        } else { None }
    };
    ALLOCATOR_LOCK.unlock();
    res
}

pub fn free_frame_safe(phys: u64) {
    ALLOCATOR_LOCK.lock();
    unsafe {
        if let Some(alloc) = (*core::ptr::addr_of_mut!(ALLOCATOR_INSTANCE)).as_mut() {
            alloc.free_frame_index((phys as usize) / PAGE_SIZE);
        }
    }
    ALLOCATOR_LOCK.unlock();
}
