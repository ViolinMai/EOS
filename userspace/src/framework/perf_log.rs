use std::fs::OpenOptions;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

pub struct SmartLogger;

static LAST_FLUSH_TICKS: AtomicU64 = AtomicU64::new(0);
static RENDERED_FRAME_COUNT: AtomicU64 = AtomicU64::new(0);
static TOTAL_FRAME_TIME_US: AtomicU64 = AtomicU64::new(0);
pub static RECENT_LOGS: Mutex<Vec<String>> = Mutex::new(Vec::new());

impl SmartLogger {
    pub fn record_frame_render(frame_cycles: u64, dirty_rects: usize) -> u64 {
        let dur_us = Self::cycles_to_us(frame_cycles);
        TOTAL_FRAME_TIME_US.fetch_add(dur_us, Ordering::Relaxed);
        let count = RENDERED_FRAME_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
        if dur_us > 16_667 {
            Self::record_operation("compositor", &format!("frame_rects_{}", dirty_rects), frame_cycles, 16_667);
        }
        count
    }

    pub fn average_frame_us() -> u64 {
        let count = RENDERED_FRAME_COUNT.load(Ordering::Relaxed);
        if count == 0 {
            0
        } else {
            TOTAL_FRAME_TIME_US.load(Ordering::Relaxed) / count
        }
    }
    #[inline(always)]
    pub fn read_tsc() -> u64 {
        let lo: u32;
        let hi: u32;
        unsafe {
            core::arch::asm!("rdtsc", out("eax") lo, out("edx") hi, options(nomem, nostack, preserves_flags));
        }
        ((hi as u64) << 32) | (lo as u64)
    }

    #[inline(always)]
    pub fn cycles_to_us(cycles: u64) -> u64 {
        cycles / 2500
    }

    pub fn record_operation(category: &'static str, target: &str, duration_cycles: u64, threshold_us: u64) {
        let dur_us = Self::cycles_to_us(duration_cycles);
        if dur_us < threshold_us {
            return;
        }

        let now = Self::read_tsc();
        let last = LAST_FLUSH_TICKS.load(Ordering::Relaxed);
        if now.saturating_sub(last) < 2_500_000 {
            return;
        }
        LAST_FLUSH_TICKS.store(now, Ordering::Relaxed);

        let sec = dur_us / 1_000_000;
        let ms = (dur_us % 1_000_000) / 1_000;
        let us = dur_us % 1_000;

        let entry = format!(
            "[BOTTLENECK] {:<12} | {:<16} | {:02}s {:03}ms {:03}us | TSC: {}\n",
            category, target, sec, ms, us, duration_cycles
        );

        if let Ok(mut logs) = RECENT_LOGS.lock() {
            if logs.len() >= 30 {
                logs.remove(0);
            }
            logs.push(entry.trim_end().to_string());
        }

        let _ = OpenOptions::new()
            .create(true)
            .append(true)
            .open("RootFS/perf_bottlenecks.log")
            .and_then(|mut f| f.write_all(entry.as_bytes()));
    }

    pub fn get_recent_entries() -> Vec<String> {
        RECENT_LOGS.lock().map(|l| l.clone()).unwrap_or_default()
    }
}
