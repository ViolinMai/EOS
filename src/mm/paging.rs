use core::arch::asm;
use crate::log_info;

pub const PAGE_SIZE: u64 = 4096;

pub const PAGE_PRESENT: u64 = 1 << 0;
pub const PAGE_WRITABLE: u64 = 1 << 1;
pub const PAGE_USER: u64 = 1 << 2;
pub const PAGE_NO_EXECUTE: u64 = 1 << 63;

const PHYSICAL_ADDRESS_MASK: u64 = 0x000F_FFFF_FFFF_F000;

#[repr(transparent)]
#[derive(Copy, Clone, Debug)]
pub struct PageTableEntry(pub u64);

impl PageTableEntry {
    pub const fn empty() -> Self {
        Self(0)
    }

    #[inline]
    pub fn is_present(&self) -> bool {
        (self.0 & PAGE_PRESENT) != 0
    }

    #[inline]
    pub fn is_writable(&self) -> bool {
        (self.0 & PAGE_WRITABLE) != 0
    }

    #[inline]
    pub fn is_user(&self) -> bool {
        (self.0 & PAGE_USER) != 0
    }

    #[inline]
    pub fn physical_address(&self) -> u64 {
        self.0 & PHYSICAL_ADDRESS_MASK
    }

    #[inline]
    pub fn set(&mut self, phys_addr: u64, flags: u64) {
        self.0 = (phys_addr & PHYSICAL_ADDRESS_MASK) | flags;
    }
}

#[repr(C, align(4096))]
pub struct PageTable {
    pub entries: [PageTableEntry; 512],
}

impl PageTable {
    pub const fn empty() -> Self {
        Self {
            entries: [PageTableEntry::empty(); 512],
        }
    }
}

pub struct VirtualMemoryManager {
    pub hhdm_offset: u64,
}

pub static mut VMM: Option<VirtualMemoryManager> = None;

impl VirtualMemoryManager {
    pub unsafe fn init(hhdm_offset: u64) {
        let cr3 = read_cr3();
        log_info!("VMM: Active PML4 Table (from CR3): {:#018x}", cr3);

        unsafe {
            *core::ptr::addr_of_mut!(VMM) = Some(Self { hhdm_offset });
        }
    }

    pub fn make_executable(&self, virt_addr: u64, size: usize) {
        let mut curr = virt_addr & !0xFFF;
        let end = (virt_addr + size as u64 + 0xFFF) & !0xFFF;

        unsafe {
            let pml4_phys = read_cr3() & PHYSICAL_ADDRESS_MASK;
            let pml4 = &mut *((pml4_phys + self.hhdm_offset) as *mut PageTable);

            while curr < end {
                let p4_idx = ((curr >> 39) & 0x1FF) as usize;
                let p3_idx = ((curr >> 30) & 0x1FF) as usize;
                let p2_idx = ((curr >> 21) & 0x1FF) as usize;
                let p1_idx = ((curr >> 12) & 0x1FF) as usize;

                if pml4.entries[p4_idx].is_present() {
                    let pdpt_phys = pml4.entries[p4_idx].physical_address();
                    let pdpt = &mut *((pdpt_phys + self.hhdm_offset) as *mut PageTable);

                    if pdpt.entries[p3_idx].is_present() {
                        let pd_phys = pdpt.entries[p3_idx].physical_address();
                        let pd = &mut *((pd_phys + self.hhdm_offset) as *mut PageTable);

                        if pd.entries[p2_idx].is_present() {
                            let pt_phys = pd.entries[p2_idx].physical_address();
                            let pt = &mut *((pt_phys + self.hhdm_offset) as *mut PageTable);

                            if pt.entries[p1_idx].is_present() {
                                // إزالة بت NX (No-Execute) وتفعيل الإذن بالتنفيذ
                                pt.entries[p1_idx].0 &= !PAGE_NO_EXECUTE;
                                pt.entries[p1_idx].0 |= PAGE_PRESENT | PAGE_WRITABLE;
                                invalidate_tlb(curr);
                            }
                        }
                    }
                }
                curr += 4096;
            }
        }
    }

    pub fn translate(&self, virt_addr: u64) -> Option<(u64, u64)> {
        let p4_idx = ((virt_addr >> 39) & 0x1FF) as usize;
        let p3_idx = ((virt_addr >> 30) & 0x1FF) as usize;
        let p2_idx = ((virt_addr >> 21) & 0x1FF) as usize;
        let p1_idx = ((virt_addr >> 12) & 0x1FF) as usize;
        let offset = virt_addr & 0xFFF;

        unsafe {
            let pml4_phys = read_cr3() & PHYSICAL_ADDRESS_MASK;
            let pml4 = &*((pml4_phys + self.hhdm_offset) as *const PageTable);
            let p4_entry = pml4.entries[p4_idx];
            if !p4_entry.is_present() {
                return None;
            }

            let pdpt_phys = p4_entry.physical_address();
            let pdpt = &*((pdpt_phys + self.hhdm_offset) as *const PageTable);
            let p3_entry = pdpt.entries[p3_idx];
            if !p3_entry.is_present() {
                return None;
            }

            if (p3_entry.0 & (1 << 7)) != 0 {
                let phys = (p3_entry.physical_address() & !0x3FFF_FFFF) | (virt_addr & 0x3FFF_FFFF);
                return Some((phys, p3_entry.0 & 0xFFF));
            }

            let pd_phys = p3_entry.physical_address();
            let pd = &*((pd_phys + self.hhdm_offset) as *const PageTable);
            let p2_entry = pd.entries[p2_idx];
            if !p2_entry.is_present() {
                return None;
            }

            if (p2_entry.0 & (1 << 7)) != 0 {
                let phys = (p2_entry.physical_address() & !0x1F_FFFF) | (virt_addr & 0x1F_FFFF);
                return Some((phys, p2_entry.0 & 0xFFF));
            }

            let pt_phys = p2_entry.physical_address();
            let pt = &*((pt_phys + self.hhdm_offset) as *const PageTable);
            let p1_entry = pt.entries[p1_idx];
            if !p1_entry.is_present() {
                return None;
            }

            let phys = p1_entry.physical_address() | offset;
            Some((phys, p1_entry.0 & 0xFFF))
        }
    }
}

#[inline]
pub fn read_cr3() -> u64 {
    let cr3: u64;
    unsafe {
        asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags));
    }
    cr3
}

#[inline]
pub unsafe fn invalidate_tlb(virt_addr: u64) {
    unsafe {
        asm!("invlpg [{}]", in(reg) virt_addr, options(nostack, preserves_flags));
    }
}
