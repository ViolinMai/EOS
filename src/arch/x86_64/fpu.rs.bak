use core::arch::asm;

pub unsafe fn enable_sse() {
    let mut cr0: u64;
    let mut cr4: u64;

    asm!("mov {}, cr0", out(reg) cr0, options(nomem, nostack));
    cr0 &= !(1 << 2); // تصفير بت EM (Emulation)
    cr0 |= 1 << 1;    // تفعيل بت MP (Monitor Coprocessor)
    asm!("mov cr0, {}", in(reg) cr0, options(nomem, nostack));

    asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack));
    cr4 |= (1 << 9) | (1 << 10); // تفعيل بت OSFXSR و بت OSXMMEXCPT
    asm!("mov cr4, {}", in(reg) cr4, options(nomem, nostack));

    asm!("fninit", options(nomem, nostack));
}
