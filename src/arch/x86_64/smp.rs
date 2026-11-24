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
        cr4 |= 1 << 9;  // OSFXSR (SSE)
        cr4 |= 1 << 10; // OSXMMEXCPT
        // Check CPUID.1:ECX.XSAVE (bit 26) and enable OSXSAVE + XCR0 (x87 + SSE + AVX)
        let cpuid1 = core::arch::x86_64::__cpuid(1);
        if (cpuid1.ecx & (1 << 26)) != 0 {
            cr4 |= 1 << 18; // OSXSAVE
            asm!("mov cr4, {}", in(reg) cr4);
            let xcr0: u64 = 0x7; // x87 (bit 0) | SSE (bit 1) | AVX (bit 2)
            asm!(
                "xsetbv",
                in("ecx") 0u32,
                in("eax") (xcr0 as u32),
                in("edx") ((xcr0 >> 32) as u32),
                options(nomem, nostack)
            );
        } else {
            asm!("mov cr4, {}", in(reg) cr4);
        }
    }
}

pub fn register_cpu(_lapic_id: u32) {
    enable_sse();
}

pub fn current_cpu_index() -> usize {
    let cpuid = core::arch::x86_64::__cpuid(1);
    ((cpuid.ebx >> 24) & 0xFF) as usize
}
