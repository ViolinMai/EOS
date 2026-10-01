use core::sync::atomic::{AtomicUsize, AtomicU64, Ordering};

pub const PROFILER_BUFFER_SIZE: usize = 2048;
pub const MAX_CORES: usize = 8;
pub const ANOMALY_THRESHOLD_US: u64 = 10_000;

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum EventType { Syscall = 1, TaskExec = 2, Interrupt = 3, MemoryAlloc = 4 }

#[derive(Copy, Clone, Debug)]
pub struct TraceEvent {
    pub tsc_start: u64, pub duration: u64, pub event_type: EventType, pub id: u64,
}

pub struct CoreProfiler {
    pub buffer: [TraceEvent; PROFILER_BUFFER_SIZE],
    pub head: AtomicUsize, pub count: AtomicUsize,
}

impl CoreProfiler {
    pub const fn new() -> Self {
        Self { buffer: [TraceEvent { tsc_start: 0, duration: 0, event_type: EventType::Syscall, id: 0 }; PROFILER_BUFFER_SIZE], head: AtomicUsize::new(0), count: AtomicUsize::new(0) }
    }
    #[inline(always)]
    pub fn record(&mut self, ev_type: EventType, id: u64, tsc_start: u64, duration: u64) {
        let idx = self.head.fetch_add(1, Ordering::Relaxed) % PROFILER_BUFFER_SIZE;
        self.buffer[idx] = TraceEvent { tsc_start, duration, event_type: ev_type, id };
        let current_count = self.count.load(Ordering::Relaxed);
        if current_count < PROFILER_BUFFER_SIZE { self.count.store(current_count + 1, Ordering::Relaxed); }
    }
}

pub static mut PROFILERS: [CoreProfiler; MAX_CORES] = [
    CoreProfiler::new(), CoreProfiler::new(), CoreProfiler::new(), CoreProfiler::new(),
    CoreProfiler::new(), CoreProfiler::new(), CoreProfiler::new(), CoreProfiler::new(),
];

#[inline(always)]
pub fn get_core_id() -> usize {
    let cpuid = core::arch::x86_64::__cpuid(1);
    ((cpuid.ebx >> 24) & 0xFF) as usize
}

#[inline(always)]
pub fn trace_event(ev_type: EventType, id: u64, tsc_start: u64, duration: u64) {
    let core_id = get_core_id();
    if core_id < MAX_CORES { unsafe { PROFILERS[core_id].record(ev_type, id, tsc_start, duration); } }
}

pub static FPS_COUNTER: AtomicUsize = AtomicUsize::new(0);
pub static LAST_FRAME_TIME_US: AtomicU64 = AtomicU64::new(0);
pub static SHOW_HUD: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
