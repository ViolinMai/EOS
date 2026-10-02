use core::arch::asm;

pub fn enable_sse() {
    unsafe {
        let mut cr0: u64;
        let mut cr4: u64;
        asm!("mov {}, cr0", out(reg) cr0);
        cr0 &= !(1 << 2); // مسح بت EM (Emulation)
        cr0 |= 1 << 1;    // تفعيل بت MP (Monitor Coprocessor)
        asm!("mov cr0, {}", in(reg) cr0);

        asm!("mov {}, cr4", out(reg) cr4);
        cr4 |= 1 << 9;  // تفعيل OSFXSR (FXSAVE/FXRSTOR و SSE تعليمات)
        cr4 |= 1 << 10; // تفعيل OSXMMEXCPT (معالجة استثناءات SIMD)
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
