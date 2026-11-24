use core::arch::asm;

pub unsafe fn enable_sse() {
    let mut cr0: u64;
    let mut cr4: u64;

    unsafe {
        asm!("mov {}, cr0", out(reg) cr0, options(nomem, nostack));
        cr0 &= !(1 << 2);
        cr0 |= 1 << 1;
        asm!("mov cr0, {}", in(reg) cr0, options(nomem, nostack));

        asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack));
        cr4 |= (1 << 9) | (1 << 10);
        let cpuid1 = core::arch::x86_64::__cpuid(1);
        if (cpuid1.ecx & (1 << 26)) != 0 {
            cr4 |= 1 << 18;
            asm!("mov cr4, {}", in(reg) cr4, options(nomem, nostack));
            asm!(
                "xsetbv",
                in("ecx") 0u32,
                in("eax") 7u32,
                in("edx") 0u32,
                options(nomem, nostack)
            );
        } else {
            asm!("mov cr4, {}", in(reg) cr4, options(nomem, nostack));
        }

        asm!("fninit", options(nomem, nostack));
    }
}
