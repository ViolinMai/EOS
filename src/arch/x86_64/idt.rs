use core::arch::asm;
use core::mem::size_of;

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct IdtEntry {
    offset_low: u16,
    pub selector: u16,
    pub ist: u8,
    pub type_attr: u8,
    offset_mid: u16,
    offset_high: u32,
    zero: u32,
}

impl IdtEntry {
    pub const fn empty() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            ist: 0,
            type_attr: 0,
            offset_mid: 0,
            offset_high: 0,
            zero: 0,
        }
    }

    pub fn set_handler(&mut self, handler: usize, selector: u16, ist: u8, type_attr: u8) {
        self.offset_low = (handler & 0xFFFF) as u16;
        self.selector = selector;
        self.ist = ist;
        self.type_attr = type_attr;
        self.offset_mid = ((handler >> 16) & 0xFFFF) as u16;
        self.offset_high = ((handler >> 32) & 0xFFFFFFFF) as u32;
        self.zero = 0;
    }
}

#[repr(C, packed)]
pub struct IdtDescriptor {
    size: u16,
    offset: u64,
}

#[repr(C, align(16))]
pub struct InterruptDescriptorTable {
    pub entries: [IdtEntry; 256],
}

impl InterruptDescriptorTable {
    pub const fn new() -> Self {
        Self {
            entries: [IdtEntry::empty(); 256],
        }
    }

    /// # Safety
    /// يجب تمرير مؤشر صالح لجدول IDT صحيح
    pub unsafe fn load_raw(ptr: *const Self) {
        unsafe {
            let descriptor = IdtDescriptor {
                size: (size_of::<Self>() - 1) as u16,
                offset: (*ptr).entries.as_ptr() as u64,
            };
            asm!("lidt [{0}]", in(reg) &descriptor, options(readonly, nostack, preserves_flags));
        }
    }
}
