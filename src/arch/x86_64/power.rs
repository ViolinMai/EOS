use core::arch::asm;
use crate::log_info;

pub fn reboot() -> ! {
    log_info!("POWER", "Initiating Q35 Hardware Reset (Port 0xCF9)...");
    unsafe {
        // تفريغ الكاش والمقاطعات
        asm!("cli", options(nomem, nostack));

        // منفذ إعادة ضبط شريحة Q35 / Intel ICH9 في QEMU
        // الكتابة بتتابع 0x02 ثم 0x06 تُطلق دورة Cold Reset حقيقية على اللوحة
        asm!("out dx, al", in("dx") 0xCF9u16, in("al") 0x02u8, options(nomem, nostack));
        for _ in 0..10_000 { core::hint::spin_loop(); }
        asm!("out dx, al", in("dx") 0xCF9u16, in("al") 0x06u8, options(nomem, nostack));

        // PS/2 Controller Reset Line كبديل
        asm!("out 0x64, al", in("al") 0xFEu8, options(nomem, nostack));

        // في حال لم تستجب الشريحة، توليد Triple Fault على النواة
        asm!("lidt [rax]", in("rax") 0, options(nomem, nostack));
        asm!("int3", options(noreturn));
    }
}

pub fn shutdown() -> ! {
    log_info!("POWER", "Shutting down system via QEMU ACPI interface...");
    unsafe {
        asm!("cli", options(nomem, nostack));
        // إشارة إطفاء الطاقة في QEMU ACPI (0x604 -> 0x2000)
        asm!("out dx, ax", in("dx") 0x604u16, in("ax") 0x2000u16, options(nomem, nostack));
        asm!("out dx, ax", in("dx") 0xB004u16, in("ax") 0x2000u16, options(nomem, nostack));
        asm!("out dx, ax", in("dx") 0x4004u16, in("ax") 0x3400u16, options(nomem, nostack));

        loop {
            asm!("hlt", options(nomem, nostack));
        }
    }
}
