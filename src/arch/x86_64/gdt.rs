use core::arch::asm;
use core::ptr::{addr_of, addr_of_mut};

pub const KERNEL_CODE_SELECTOR: u16 = 0x08;
#[allow(dead_code)]
pub const KERNEL_DATA_SELECTOR: u16 = 0x10;
#[allow(dead_code)]
pub const USER_DATA_SELECTOR: u16 = 0x18 | 3;
#[allow(dead_code)]
pub const USER_CODE_SELECTOR: u16 = 0x20 | 3;
pub const TSS_SELECTOR: u16 = 0x28;

#[repr(C, packed)]
pub struct TaskStateSegment {
    reserved1: u32,
    pub rsp0: u64,
    pub rsp1: u64,
    pub rsp2: u64,
    reserved2: u64,
    pub ist1: u64,
    pub ist2: u64,
    pub ist3: u64,
    pub ist4: u64,
    pub ist5: u64,
    pub ist6: u64,
    pub ist7: u64,
    reserved3: u64,
    reserved4: u16,
    pub iomap_base: u16,
}

impl TaskStateSegment {
    pub const fn zeroed() -> Self {
        Self {
            reserved1: 0,
            rsp0: 0,
            rsp1: 0,
            rsp2: 0,
            reserved2: 0,
            ist1: 0,
            ist2: 0,
            ist3: 0,
            ist4: 0,
            ist5: 0,
            ist6: 0,
            ist7: 0,
            reserved3: 0,
            reserved4: 0,
            iomap_base: core::mem::size_of::<TaskStateSegment>() as u16,
        }
    }
}

pub static mut TSS: TaskStateSegment = TaskStateSegment::zeroed();

#[repr(C, align(16))]
pub struct Gdt {
    pub entries: [u64; 8],
}

pub static mut GDT: Gdt = Gdt { entries: [0; 8] };

#[repr(C, packed)]
struct GdtDescriptor {
    limit: u16,
    base: u64,
}

static mut EMERGENCY_STACK: [u8; 8192] = [0; 8192];
static mut KERNEL_SYSCALL_STACK: [u8; 8192] = [0; 8192];

pub fn init() {
    unsafe {
        let stack_top = addr_of_mut!(EMERGENCY_STACK) as *mut u8 as u64 + 8192;
        let syscall_stack_top = addr_of_mut!(KERNEL_SYSCALL_STACK) as *mut u8 as u64 + 8192;

        TSS.ist1 = stack_top;
        TSS.rsp0 = syscall_stack_top;

        GDT.entries[0] = 0;
        GDT.entries[1] = 0x00AF9A000000FFFF;
        GDT.entries[2] = 0x00CF92000000FFFF;
        GDT.entries[3] = 0x00CFF2000000FFFF;
        GDT.entries[4] = 0x00AFFA000000FFFF;

        let tss_base = addr_of!(TSS) as u64;
        let tss_limit = (core::mem::size_of::<TaskStateSegment>() - 1) as u64;

        let mut tss_low = 0x0000890000000000u64;
        tss_low |= tss_limit & 0xFFFF;
        tss_low |= (tss_base & 0xFFFFFF) << 16;
        tss_low |= (tss_base & 0xFF000000) << 32;

        let tss_high = tss_base >> 32;

        GDT.entries[5] = tss_low;
        GDT.entries[6] = tss_high;
        GDT.entries[7] = 0;

        let desc = GdtDescriptor {
            limit: (core::mem::size_of::<Gdt>() - 1) as u16,
            base: addr_of!(GDT) as u64,
        };

        asm!(
            "lgdt [{desc}]",
            "push 0x08",
            "lea rax, [2f + rip]",
            "push rax",
            "retfq",
            "2:",
            "mov ax, 0x10",
            "mov ds, ax",
            "mov es, ax",
            "mov fs, ax",
            "mov gs, ax",
            "mov ss, ax",
            desc = in(reg) &desc,
            out("rax") _,
            options(nomem, preserves_flags)
        );

        asm!("ltr cx", in("cx") TSS_SELECTOR, options(nomem, nostack, preserves_flags));
    }
}
