use core::arch::asm;
use core::ptr::addr_of_mut;

pub const KERNEL_CODE_SELECTOR: u16 = 0x08;
#[allow(dead_code)]
pub const KERNEL_DATA_SELECTOR: u16 = 0x10;
pub const USER_CODE_SELECTOR: u16 = 0x18 | 3;
pub const USER_DATA_SELECTOR: u16 = 0x20 | 3;
pub const TSS_SELECTOR: u16 = 0x28;

pub const MAX_CORES: usize = 8;

#[repr(C, packed)]
#[derive(Copy, Clone)]
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

#[derive(Copy, Clone)]
#[repr(C, align(16))]
pub struct Gdt {
    pub entries: [u64; 8],
}

#[repr(C, packed)]
struct GdtDescriptor {
    limit: u16,
    base: u64,
}

#[derive(Copy, Clone)]
#[repr(align(16))]
struct CoreStacks {
    emergency: [u8; 8192],
    syscall: [u8; 8192],
}

static mut GDTS: [Gdt; MAX_CORES] = [Gdt { entries: [0; 8] }; MAX_CORES];
static mut TSSES: [TaskStateSegment; MAX_CORES] = [TaskStateSegment::zeroed(); MAX_CORES];
static mut STACKS: [CoreStacks; MAX_CORES] = [CoreStacks { emergency: [0; 8192], syscall: [0; 8192] }; MAX_CORES];

pub fn init_core(core_id: usize) {
    if core_id >= MAX_CORES {
        return;
    }

    unsafe {
        let tss = &mut (*addr_of_mut!(TSSES))[core_id];
        let st = &mut (*addr_of_mut!(STACKS))[core_id];

        let stack_top = st.emergency.as_ptr() as u64 + 8192;
        let syscall_stack_top = st.syscall.as_ptr() as u64 + 8192;

        tss.ist1 = stack_top;
        tss.rsp0 = syscall_stack_top;

        crate::arch::x86_64::syscall::KERNEL_SYSCALL_STACKS[core_id] = syscall_stack_top;

        let gdt = &mut (*addr_of_mut!(GDTS))[core_id];
        gdt.entries[0] = 0;
        gdt.entries[1] = 0x00AF9A000000FFFF; // Kernel Code (0x08)
        gdt.entries[2] = 0x00CF92000000FFFF; // Kernel Data (0x10)
        
        // 💡 للـ SYSRET: يجب أن يكون User Code أولاً ثم User Data مباشرة
        gdt.entries[3] = 0x00AFFA000000FFFF; // User Code (0x18 | 3)
        gdt.entries[4] = 0x00CFF2000000FFFF; // User Data (0x20 | 3)

        let tss_base = tss as *const _ as u64;
        let tss_limit = (core::mem::size_of::<TaskStateSegment>() - 1) as u64;

        let mut tss_low = 0x0000890000000000u64;
        tss_low |= tss_limit & 0xFFFF;
        tss_low |= (tss_base & 0xFFFFFF) << 16;
        tss_low |= (tss_base & 0xFF000000) << 32;

        gdt.entries[5] = tss_low;
        gdt.entries[6] = tss_base >> 32;
        gdt.entries[7] = 0;

        let desc = GdtDescriptor {
            limit: (core::mem::size_of::<Gdt>() - 1) as u16,
            base: gdt as *const _ as u64,
        };

        asm!(
            "sub rsp, 128",
            "lgdt [{0}]",
            "push 0x08",
            "lea rax, [rip + 2f]",
            "push rax",
            "retfq",
            "2:",
            "add rsp, 128",
            "mov ax, 0x10",
            "mov ds, ax",
            "mov es, ax",
            "mov fs, ax",
            "mov gs, ax",
            "mov ss, ax",
            in(reg) &desc,
            out("rax") _,
        );

        asm!("ltr cx", in("cx") TSS_SELECTOR, options(nostack, preserves_flags));
    }
}
