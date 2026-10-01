use core::sync::atomic::{AtomicUsize, Ordering};
use core::arch::asm;
use crate::config::CONFIG;

pub fn enable_sse() {
    unsafe {
        let mut cr0: u64; let mut cr4: u64;
        asm!("mov {}, cr0", out(reg) cr0);
        cr0 &= !(1 << 2); cr0 |= 1 << 1;
        asm!("mov cr0, {}", in(reg) cr0);
        asm!("mov {}, cr4", out(reg) cr4);
        cr4 |= 1 << 9; cr4 |= 1 << 10;
        asm!("mov cr4, {}", in(reg) cr4);
    }
}

pub fn register_cpu(_lapic_id: u32) {
    enable_sse();
}

pub fn current_cpu_index() -> usize {
    let cpuid = core::arch::x86_64::__cpuid(1);
    ((cpuid.ebx >> 24) & 0xFF) as usize
}
