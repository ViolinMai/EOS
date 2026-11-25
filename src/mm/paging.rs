use core::arch::asm;
use core::ptr::addr_of;
use crate::log_info;

#[allow(dead_code)]
pub const PAGE_SIZE: u64 = 4096;
pub const PAGE_PRESENT: u64 = 1 << 0;
pub const PAGE_WRITABLE: u64 = 1 << 1;
pub const PAGE_USER: u64 = 1 << 2;
pub const PAGE_WRITE_THROUGH: u64 = 1 << 3;
pub const PAGE_CACHE_DISABLE: u64 = 1 << 4;
pub const PAGE_HUGE: u64 = 1 << 7;
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
        // Configure IA32_PAT MSR 0x277 so PAT1 = Write-Combining (0x01) globally
        unsafe {
            crate::arch::x86_64::syscall::wrmsr(0x277, 0x0007_0106_0007_0106);
        }
        log_info!("VMM: Active PML4 Table (from CR3): {:#018x} (IA32_PAT WC Active)", cr3);
        unsafe { *core::ptr::addr_of_mut!(VMM) = Some(Self { hhdm_offset }); }
    }
}

/// Milestone 27: Configure IA32_PAT (MSR 0x277) so PAT1 (PWT=1, PCD=0, PAT=0) = Write-Combining (0x01)
/// and mark linear framebuffer pages as Write-Combining for full bare-metal blit bandwidth.
pub fn init_pat_wc_framebuffer(fb_virt: u64, size_bytes: usize) -> bool {
    unsafe {
        // PAT0=WB(06), PAT1=WC(01), PAT2=UC-(07), PAT3=UC(00), PAT4=WB(06), PAT5=WC(01), PAT6=UC-(07), PAT7=UC(00)
        let pat_val: u64 = 0x0007_0106_0007_0106;
        crate::arch::x86_64::syscall::wrmsr(0x277, pat_val);

        let vmm = match (*addr_of!(VMM)).as_ref() {
            Some(v) => v,
            None => return false,
        };
        let pml4_phys = read_cr3() & PHYSICAL_ADDRESS_MASK;
        let pml4 = &mut *((pml4_phys + vmm.hhdm_offset) as *mut PageTable);

        let start = fb_virt & !0xFFF;
        let end = (fb_virt.saturating_add(size_bytes as u64) + 0xFFF) & !0xFFF;
        let mut curr = start;
        let mut updated_pages = 0usize;

        while curr < end {
            let p4_idx = ((curr >> 39) & 0x1FF) as usize;
            if !pml4.entries[p4_idx].is_present() { curr += 4096; continue; }
            let pt3 = &mut *((pml4.entries[p4_idx].physical_address() + vmm.hhdm_offset) as *mut PageTable);

            let p3_idx = ((curr >> 30) & 0x1FF) as usize;
            if !pt3.entries[p3_idx].is_present() { curr += 4096; continue; }
            if (pt3.entries[p3_idx].0 & PAGE_HUGE) != 0 {
                pt3.entries[p3_idx].0 = (pt3.entries[p3_idx].0 & !PAGE_CACHE_DISABLE) | PAGE_WRITE_THROUGH;
                invalidate_tlb(curr);
                updated_pages += 1;
                curr = (curr + 0x4000_0000) & !0x3FFF_FFFF;
                continue;
            }
            let pt2 = &mut *((pt3.entries[p3_idx].physical_address() + vmm.hhdm_offset) as *mut PageTable);

            let p2_idx = ((curr >> 21) & 0x1FF) as usize;
            if !pt2.entries[p2_idx].is_present() { curr += 4096; continue; }
            if (pt2.entries[p2_idx].0 & PAGE_HUGE) != 0 {
                pt2.entries[p2_idx].0 = (pt2.entries[p2_idx].0 & !PAGE_CACHE_DISABLE) | PAGE_WRITE_THROUGH;
                invalidate_tlb(curr);
                updated_pages += 1;
                curr = (curr + 0x20_0000) & !0x1F_FFFF;
                continue;
            }
            let pt1 = &mut *((pt2.entries[p2_idx].physical_address() + vmm.hhdm_offset) as *mut PageTable);

            let p1_idx = ((curr >> 12) & 0x1FF) as usize;
            if pt1.entries[p1_idx].is_present() {
                pt1.entries[p1_idx].0 = (pt1.entries[p1_idx].0 & !PAGE_CACHE_DISABLE) | PAGE_WRITE_THROUGH;
                invalidate_tlb(curr);
                updated_pages += 1;
            }
            curr += 4096;
        }
        log_info!("PAT-WC", "IA32_PAT configured; {} framebuffer page entries mapped as Write-Combining.", updated_pages);
        updated_pages > 0
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
    let vmm = match unsafe { (*addr_of!(VMM)).as_ref() } {
        Some(v) => v,
        None => return,
    };
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

pub fn map_mmio_pages(phys_addr: u64, pages: usize) -> Result<u64, &'static str> {
    let vmm = unsafe { (*addr_of!(VMM)).ok_or("VMM not initialized")? };
    let pml4_phys = read_cr3() & PHYSICAL_ADDRESS_MASK;
    let pml4 = unsafe { &mut *((pml4_phys + vmm.hhdm_offset) as *mut PageTable) };
    let table_flags = PAGE_PRESENT | PAGE_WRITABLE;
    let mmio_flags = PAGE_PRESENT | PAGE_WRITABLE | PAGE_CACHE_DISABLE;

    let base_phys = phys_addr & !0xFFF;
    for i in 0..pages {
        let curr_phys = base_phys + (i as u64 * 4096);
        let curr_vaddr = curr_phys + vmm.hhdm_offset;

        let p4_idx = ((curr_vaddr >> 39) & 0x1FF) as usize;
        let p3_idx = ((curr_vaddr >> 30) & 0x1FF) as usize;
        let p2_idx = ((curr_vaddr >> 21) & 0x1FF) as usize;
        let p1_idx = ((curr_vaddr >> 12) & 0x1FF) as usize;

        if !pml4.entries[p4_idx].is_present() {
            let f = crate::mm::frame::allocate_frame_safe().ok_or("OOM: MMIO PT4")?;
            unsafe { core::ptr::write_bytes((f + vmm.hhdm_offset) as *mut u8, 0, 4096); }
            pml4.entries[p4_idx].set(f, table_flags);
        }
        let pt4 = unsafe { &mut *((pml4.entries[p4_idx].physical_address() + vmm.hhdm_offset) as *mut PageTable) };

        if !pt4.entries[p3_idx].is_present() {
            let f = crate::mm::frame::allocate_frame_safe().ok_or("OOM: MMIO PT3")?;
            unsafe { core::ptr::write_bytes((f + vmm.hhdm_offset) as *mut u8, 0, 4096); }
            pt4.entries[p3_idx].set(f, table_flags);
        } else if (pt4.entries[p3_idx].0 & PAGE_HUGE) != 0 {
            continue;
        }
        let pt3 = unsafe { &mut *((pt4.entries[p3_idx].physical_address() + vmm.hhdm_offset) as *mut PageTable) };

        if !pt3.entries[p2_idx].is_present() {
            let f = crate::mm::frame::allocate_frame_safe().ok_or("OOM: MMIO PT2")?;
            unsafe { core::ptr::write_bytes((f + vmm.hhdm_offset) as *mut u8, 0, 4096); }
            pt3.entries[p2_idx].set(f, table_flags);
        } else if (pt3.entries[p2_idx].0 & PAGE_HUGE) != 0 {
            continue;
        }
        let pt2 = unsafe { &mut *((pt3.entries[p2_idx].physical_address() + vmm.hhdm_offset) as *mut PageTable) };

        pt2.entries[p1_idx].set(curr_phys, mmio_flags);
        unsafe { invalidate_tlb(curr_vaddr); }
    }
    Ok(phys_addr + vmm.hhdm_offset)
}

/// Real `munmap` implementation: unmaps user pages, frees physical frames back to PMM, and returns freed bytes.
pub fn unmap_user_pages(pml4_phys: u64, virt_addr: u64, pages: usize) -> usize {
    let vmm = match unsafe { (*addr_of!(VMM)).as_ref() } {
        Some(v) => v,
        None => return 0,
    };
    let pml4 = unsafe { &mut *((pml4_phys + vmm.hhdm_offset) as *mut PageTable) };
    let mut curr_vaddr = virt_addr & !0xFFF;
    let mut freed_bytes = 0usize;

    for _ in 0..pages {
        if curr_vaddr >= 0x0000_8000_0000_0000 { break; }
        let p4_idx = ((curr_vaddr >> 39) & 0x1FF) as usize;
        let p3_idx = ((curr_vaddr >> 30) & 0x1FF) as usize;
        let p2_idx = ((curr_vaddr >> 21) & 0x1FF) as usize;
        let p1_idx = ((curr_vaddr >> 12) & 0x1FF) as usize;

        if pml4.entries[p4_idx].is_present() {
            let pt3 = unsafe { &mut *((pml4.entries[p4_idx].physical_address() + vmm.hhdm_offset) as *mut PageTable) };
            if pt3.entries[p3_idx].is_present() {
                let pt2 = unsafe { &mut *((pt3.entries[p3_idx].physical_address() + vmm.hhdm_offset) as *mut PageTable) };
                if pt2.entries[p2_idx].is_present() {
                    let pt1 = unsafe { &mut *((pt2.entries[p2_idx].physical_address() + vmm.hhdm_offset) as *mut PageTable) };
                    if pt1.entries[p1_idx].is_present() && pt1.entries[p1_idx].is_user() {
                        let phys = pt1.entries[p1_idx].physical_address();
                        pt1.entries[p1_idx] = PageTableEntry::empty();
                        unsafe { invalidate_tlb(curr_vaddr); }
                        crate::mm::frame::free_frame_safe(phys);
                        freed_bytes += 4096;
                    }
                }
            }
        }
        curr_vaddr += 4096;
    }
    freed_bytes
}

pub fn is_user_mapped(pml4_phys: u64, virt_addr: u64) -> bool {
    unsafe {
        let vmm = if let Some(v) = (*core::ptr::addr_of!(VMM)).as_ref() { v } else { return false; };
        let pml4 = &*((pml4_phys + vmm.hhdm_offset) as *const PageTable);
        let p4_idx = ((virt_addr >> 39) & 0x1FF) as usize;
        if !pml4.entries[p4_idx].is_present() { return false; }

        let pt3 = &*((pml4.entries[p4_idx].physical_address() + vmm.hhdm_offset) as *const PageTable);
        let p3_idx = ((virt_addr >> 30) & 0x1FF) as usize;
        if !pt3.entries[p3_idx].is_present() { return false; }

        let pt2 = &*((pt3.entries[p3_idx].physical_address() + vmm.hhdm_offset) as *const PageTable);
        let p2_idx = ((virt_addr >> 21) & 0x1FF) as usize;
        if !pt2.entries[p2_idx].is_present() { return false; }

        let pt1 = &*((pt2.entries[p2_idx].physical_address() + vmm.hhdm_offset) as *const PageTable);
        let p1_idx = ((virt_addr >> 12) & 0x1FF) as usize;
        pt1.entries[p1_idx].is_present() && pt1.entries[p1_idx].is_user()
    }
}
