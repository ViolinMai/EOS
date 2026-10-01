use core::arch::asm;
use core::sync::atomic::{AtomicU64, Ordering};

const PIT_CHANNEL_0: u16 = 0x40;
const PIT_COMMAND: u16 = 0x43;
const PIT_BASE_FREQUENCY: u32 = 1193182;
pub const TARGET_HZ: u32 = 1000; // Raised to 1000 Hz (1ms tick)

pub static TICKS: AtomicU64 = AtomicU64::new(0);
pub static TSC_PER_MS: AtomicU64 = AtomicU64::new(0);

#[inline]
unsafe fn outb(port: u16, val: u8) {
    unsafe { asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags)); }
}

pub unsafe fn init() {
    let divisor: u16 = (PIT_BASE_FREQUENCY / TARGET_HZ) as u16;
    unsafe {
        outb(PIT_COMMAND, 0x34);
        outb(PIT_CHANNEL_0, (divisor & 0xFF) as u8);
        outb(PIT_CHANNEL_0, ((divisor >> 8) & 0xFF) as u8);
    }
}

#[inline]
pub fn on_tick() {
    TICKS.fetch_add(1, Ordering::Relaxed);
}

pub fn get_ticks() -> u64 { TICKS.load(Ordering::Relaxed) }
pub fn get_uptime_seconds() -> u64 { get_ticks() / (TARGET_HZ as u64) }

#[inline]
pub fn read_tsc() -> u64 {
    let lo: u32; let hi: u32;
    unsafe { asm!("rdtsc", out("eax") lo, out("edx") hi, options(nomem, nostack, preserves_flags)); }
    ((hi as u64) << 32) | (lo as u64)
}

pub fn calibrate_tsc() {
    let start_tick = get_ticks();
    while get_ticks() == start_tick { core::hint::spin_loop(); }
    let t1 = read_tsc();
    let tick1 = get_ticks();
    while get_ticks() - tick1 < 10 { core::hint::spin_loop(); }
    let t2 = read_tsc();
    TSC_PER_MS.store((t2 - t1) / 10, Ordering::SeqCst);
    crate::log_info!("PIT", "TSC Calibrated: {} ticks/ms", (t2 - t1) / 10);
}

pub fn sleep_us(us: u64) {
    let tsc_per_ms = TSC_PER_MS.load(Ordering::Relaxed);
    if tsc_per_ms == 0 { return; } // Not calibrated yet
    let target = read_tsc() + (us * tsc_per_ms) / 1000;
    unsafe { asm!("sti", options(nomem, nostack)); }
    while read_tsc() < target {
        unsafe { asm!("pause", options(nomem, nostack)); }
    }
}

pub fn sleep_ms(ms: u64) {
    sleep_us(ms * 1000);
}
