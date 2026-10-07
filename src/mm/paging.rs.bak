use core::arch::asm;
use core::ptr::addr_of;
use crate::log_info;

#[allow(dead_code)]
pub const PAGE_SIZE: u64 = 4096;
pub const PAGE_PRESENT: u64 = 1 << 0;
pub const PAGE_WRITABLE: u64 = 1 << 1;
pub const PAGE_USER: u64 = 1 << 2;
#[allow(dead_code)]
pub const PAGE_NO_EXECUTE: u64 = 1 << 63;
const PHYSICAL_ADDRESS_MASK: u64 = 0x000F_FFFF_FFFF_F000;

#[repr(transparent)]
#[derive(Copy, Clone, Debug)]
pub struct PageTableEntry(pub u64);

impl PageTableEntry {
    #[allow(dead_code)]
    pub const fn empty() -> Self { Self(0) }
    #[inline]
    pub fn is_present(&self) -> bool { (self.0 & PAGE_PRESENT) != 0 }
    #[allow(dead_code)]
    #[inline]
    pub fn is_writable(&self) -> bool { (self.0 & PAGE_WRITABLE) != 0 }
    #[allow(dead_code)]
    #[inline]
    pub fn is_user(&self) -> bool { (self.0 & PAGE_USER) != 0 }
    #[inline]
    pub fn physical_address(&self) -> u64 { self.0 & PHYSICAL_ADDRESS_MASK }
    #[inline]
    pub fn set(&mut self, phys_addr: u64, flags: u64) { self.0 = (phys_addr & PHYSICAL_ADDRESS_MASK) | flags; }
}

#[repr(C, align(4096))]
pub struct PageTable { pub entries: [PageTableEntry; 512], }

impl PageTable {
    #[allow(dead_code)]
    pub const fn empty() -> Self { Self { entries: [PageTableEntry::empty(); 512] } }
}

#[derive(Copy, Clone)]
pub struct VirtualMemoryManager { pub hhdm_offset: u64, }
pub static mut VMM: Option<VirtualMemoryManager> = None;

impl VirtualMemoryManager {
    pub unsafe fn init(hhdm_offset: u64) {
        let cr3 = read_cr3();
        log_info!("VMM: Active PML4 Table (from CR3): {:#018x}", cr3);
        unsafe { *core::ptr::addr_of_mut!(VMM) = Some(Self { hhdm_offset }); }
    }
}

pub fn map_user_pages(pml4_phys: u64, virt_addr: u64, pages: usize) -> Result<(), &'static str> {
    let vmm = unsafe { (*addr_of!(VMM)).ok_or("VMM not initialized")? };
    let user_flags = PAGE_PRESENT | PAGE_WRITABLE | PAGE_USER;
    let pml4 = unsafe { &mut *((pml4_phys + vmm.hhdm_offset) as *mut PageTable) };

    let mut curr_vaddr = virt_addr & !0xFFF;
    for _ in 0..pages {
        let p4_idx = ((curr_vaddr >> 39) & 0x1FF) as usize;
        let p3_idx = ((curr_vaddr >> 30) & 0x1FF) as usize;
        let p2_idx = ((curr_vaddr >> 21) & 0x1FF) as usize;
        let p1_idx = ((curr_vaddr >> 12) & 0x1FF) as usize;

        if !pml4.entries[p4_idx].is_present() {
            let f = crate::mm::frame::allocate_frame_safe().ok_or("OOM: PT4")?;
            unsafe { core::ptr::write_bytes((f + vmm.hhdm_offset) as *mut u8, 0, 4096); }
            pml4.entries[p4_idx].set(f, user_flags);
        }
        let pt4 = unsafe { &mut *((pml4.entries[p4_idx].physical_address() + vmm.hhdm_offset) as *mut PageTable) };

        if !pt4.entries[p3_idx].is_present() {
            let f = crate::mm::frame::allocate_frame_safe().ok_or("OOM: PT3")?;
            unsafe { core::ptr::write_bytes((f + vmm.hhdm_offset) as *mut u8, 0, 4096); }
            pt4.entries[p3_idx].set(f, user_flags);
        }
        let pt3 = unsafe { &mut *((pt4.entries[p3_idx].physical_address() + vmm.hhdm_offset) as *mut PageTable) };

        if !pt3.entries[p2_idx].is_present() {
            let f = crate::mm::frame::allocate_frame_safe().ok_or("OOM: PT2")?;
            unsafe { core::ptr::write_bytes((f + vmm.hhdm_offset) as *mut u8, 0, 4096); }
            pt3.entries[p2_idx].set(f, user_flags);
        }
        let pt2 = unsafe { &mut *((pt3.entries[p2_idx].physical_address() + vmm.hhdm_offset) as *mut PageTable) };

        if !pt2.entries[p1_idx].is_present() {
            let frame_phys = crate::mm::frame::allocate_frame_safe().ok_or("OOM: Page Alloc")?;
            let frame_ptr = (frame_phys + vmm.hhdm_offset) as *mut u8;
            unsafe { core::ptr::write_bytes(frame_ptr, 0, 4096); }
            pt2.entries[p1_idx].set(frame_phys, user_flags);
            unsafe { invalidate_tlb(curr_vaddr); }
        }
        curr_vaddr += 4096;
    }
    Ok(())
}

pub unsafe fn free_user_pages(pml4_phys: u64) {
    let vmm = unsafe { (*addr_of!(VMM)).unwrap() };
    let pml4 = unsafe { &mut *((pml4_phys + vmm.hhdm_offset) as *mut PageTable) };
    
    for p4 in 0..256 {
        if pml4.entries[p4].is_present() {
            let pt3_phys = pml4.entries[p4].physical_address();
            let pt3 = unsafe { &mut *((pt3_phys + vmm.hhdm_offset) as *mut PageTable) };
            for p3 in 0..512 {
                if pt3.entries[p3].is_present() {
                    let pt2_phys = pt3.entries[p3].physical_address();
                    let pt2 = unsafe { &mut *((pt2_phys + vmm.hhdm_offset) as *mut PageTable) };
                    for p2 in 0..512 {
                        if pt2.entries[p2].is_present() {
                            let pt1_phys = pt2.entries[p2].physical_address();
                            let pt1 = unsafe { &mut *((pt1_phys + vmm.hhdm_offset) as *mut PageTable) };
                            for p1 in 0..512 {
                                if pt1.entries[p1].is_present() {
                                    crate::mm::frame::free_frame_safe(pt1.entries[p1].physical_address());
                                }
                            }
                            crate::mm::frame::free_frame_safe(pt1_phys);
                        }
                    }
                    crate::mm::frame::free_frame_safe(pt2_phys);
                }
            }
            crate::mm::frame::free_frame_safe(pt3_phys);
        }
    }
    crate::mm::frame::free_frame_safe(pml4_phys);
}

#[inline]
pub fn read_cr3() -> u64 {
    let cr3: u64;
    unsafe { asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags)); }
    cr3
}

#[inline]
pub unsafe fn invalidate_tlb(virt_addr: u64) {
    unsafe { asm!("invlpg [{}]", in(reg) virt_addr, options(nostack, preserves_flags)); }
}
