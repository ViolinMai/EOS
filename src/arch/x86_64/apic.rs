use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicU64, Ordering};

const IA32_APIC_BASE_MSR: u32 = 0x1B;
const LAPIC_REG_EOI: usize = 0x0B0;
const LAPIC_REG_SVR: usize = 0x0F0;
const LAPIC_REG_ICR_LO: usize = 0x300;
const LAPIC_REG_ICR_HI: usize = 0x310;
const LAPIC_REG_LVT_TIMER: usize = 0x320;
const LAPIC_REG_TIMER_INIT: usize = 0x380;
const LAPIC_REG_TIMER_DIV: usize = 0x3E0;

pub const LAPIC_TIMER_VECTOR: u8 = 48;
pub const LAPIC_SPURIOUS_VECTOR: u8 = 0xFF;

static LAPIC_VIRT_BASE: AtomicU64 = AtomicU64::new(0);

/// Called once on Core 0 (BSP) before APs boot so every kernel & user PML4 inherits the LAPIC MMIO mapping.
pub fn init_bsp_mmio() {
    let apic_msr = unsafe { crate::arch::x86_64::syscall::rdmsr(IA32_APIC_BASE_MSR) };
    let apic_phys = apic_msr & 0x000F_FFFF_FFFF_F000;
    if apic_phys == 0 {
        return;
    }
    if let Ok(virt) = crate::mm::paging::map_mmio_pages(apic_phys, 1) {
        LAPIC_VIRT_BASE.store(virt, Ordering::Release);
        unsafe {
            crate::arch::x86_64::syscall::wrmsr(IA32_APIC_BASE_MSR, apic_msr | (1 << 11));
            let svr_ptr = (virt as usize + LAPIC_REG_SVR) as *mut u32;
            write_volatile(svr_ptr, (read_volatile(svr_ptr) & !0xFF) | 0x100 | (LAPIC_SPURIOUS_VECTOR as u32));
        }
        crate::log_info!("APIC", "Local APIC MMIO mapped at {:#x} (phys {:#x})", virt, apic_phys);
    }
}

/// Called on each AP core (Cores 1..7) to enable its private periodic hardware timer (~100Hz / 10ms)
/// so `sti; hlt` safely sleeps and wakes without burning host CPU cycles.
pub fn init_ap_timer() {
    let base = LAPIC_VIRT_BASE.load(Ordering::Acquire);
    if base == 0 {
        return;
    }
    unsafe {
        let msr = crate::arch::x86_64::syscall::rdmsr(IA32_APIC_BASE_MSR);
        crate::arch::x86_64::syscall::wrmsr(IA32_APIC_BASE_MSR, msr | (1 << 11));

        let svr_ptr = (base as usize + LAPIC_REG_SVR) as *mut u32;
        write_volatile(svr_ptr, (read_volatile(svr_ptr) & !0xFF) | 0x100 | (LAPIC_SPURIOUS_VECTOR as u32));

        // Divide by 16 -> on QEMU 1GHz APIC bus = 62.5MHz; count 625_000 = 10ms periodic wakeup
        write_volatile((base as usize + LAPIC_REG_TIMER_DIV) as *mut u32, 0x3);
        write_volatile(
            (base as usize + LAPIC_REG_LVT_TIMER) as *mut u32,
            (1 << 17) | (LAPIC_TIMER_VECTOR as u32),
        );
        write_volatile((base as usize + LAPIC_REG_TIMER_INIT) as *mut u32, 625_000);
    }
}

/// Sends an instant Inter-Processor Interrupt (IPI) to wake a halted target core (0ms input latency).
#[inline]
pub fn send_ipi(target_lapic_id: u8, vector: u8) {
    let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
    if base == 0 {
        return;
    }
    unsafe {
        let icr_lo = (base as usize + LAPIC_REG_ICR_LO) as *mut u32;
        let icr_hi = (base as usize + LAPIC_REG_ICR_HI) as *mut u32;
        let mut guard = 0u32;
        while (read_volatile(icr_lo) & (1 << 12)) != 0 && guard < 10_000 {
            core::hint::spin_loop();
            guard += 1;
        }
        write_volatile(icr_hi, (target_lapic_id as u32) << 24);
        write_volatile(icr_lo, (vector as u32) | (1 << 14));
    }
}

#[inline(always)]
pub fn is_active() -> bool {
    LAPIC_VIRT_BASE.load(Ordering::Relaxed) != 0
}

#[inline(always)]
pub fn eoi() {
    let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
    if base != 0 {
        unsafe {
            write_volatile((base as usize + LAPIC_REG_EOI) as *mut u32, 0);
        }
    }
}
