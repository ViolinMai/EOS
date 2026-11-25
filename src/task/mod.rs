pub mod context;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use crate::arch::x86_64::interrupts::IS_RUNNING_PROGRAM;
use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering};

pub const TASK_STACK_SIZE: usize = 16 * 1024;

#[derive(Clone, Debug)]
pub enum FileSource {
    Memory(Vec<u8>),
    AtaDisk { drive: u8, first_cluster: u32, size: usize },
    Directory(Vec<crate::fs::FsItem>, usize),
    Pipe { id: usize, is_read: bool },
}

#[derive(Clone)]
pub struct FileDescriptor {
    pub path: String,
    pub source: FileSource,
    pub offset: usize,
    pub is_writable: bool,
    pub is_dirty: bool,
}

#[repr(C)]
pub struct Task { pub id: u64, pub rsp: u64, pub name: &'static str, pub counter: u64, stack: Box<[u8; TASK_STACK_SIZE]>, pub mmap_bump: u64, pub fd_table: [Option<FileDescriptor>; 32] }

impl Task {
    pub fn new(id: u64, name: &'static str, entry: extern "C" fn()) -> Self {
        let stack = Box::new([0u8; TASK_STACK_SIZE]);
        let stack_top = stack.as_ptr() as u64 + TASK_STACK_SIZE as u64;
        let mut current_sp = stack_top;
        unsafe {
            current_sp &= !0xF; current_sp -= 512;
            current_sp -= 8; *(current_sp as *mut u64) = 0x10;
            current_sp -= 8; *(current_sp as *mut u64) = stack_top - 512;
            current_sp -= 8; *(current_sp as *mut u64) = 0x202;
            current_sp -= 8; *(current_sp as *mut u64) = 0x08;
            current_sp -= 8; *(current_sp as *mut u64) = entry as usize as u64;
            for _ in 0..15 { current_sp -= 8; *(current_sp as *mut u64) = 0; }
        }
        Self { id, rsp: current_sp, name, counter: 0, stack, mmap_bump: 0x0000_0001_0000_0000, fd_table: Default::default() }
    }
}

pub struct TaskScheduler { pub tasks: Vec<Task>, pub current_task_idx: usize, pub preemptive_enabled: bool }
pub static mut SCHEDULER: Option<TaskScheduler> = None;

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum JobState { Idle, Submitted, Running, Finished, Failed }

pub struct PerCoreJob { pub task_fn: Option<fn()>, pub state: JobState, pub result: u64 }
static JOB_LOCKS: [AtomicBool; 8] = [AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false)];
pub static mut CORE_JOBS: [PerCoreJob; 8] = [PerCoreJob { task_fn: None, state: JobState::Idle, result: 0 }, PerCoreJob { task_fn: None, state: JobState::Idle, result: 0 }, PerCoreJob { task_fn: None, state: JobState::Idle, result: 0 }, PerCoreJob { task_fn: None, state: JobState::Idle, result: 0 }, PerCoreJob { task_fn: None, state: JobState::Idle, result: 0 }, PerCoreJob { task_fn: None, state: JobState::Idle, result: 0 }, PerCoreJob { task_fn: None, state: JobState::Idle, result: 0 }, PerCoreJob { task_fn: None, state: JobState::Idle, result: 0 }];

pub static mut ELF_TARGET_FILES: [[u8; 128]; 8] = [[0; 128]; 8];
pub static mut ELF_TARGET_ARGS: [[u8; 128]; 8] = [[0; 128]; 8];
pub static ELF_ACTIVE_RUNNING: AtomicBool = AtomicBool::new(false);
pub static CORE_ELF_SPAWN_REQ: [AtomicBool; 8] = [AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false)];
pub static CORE_IS_BUSY: [AtomicBool; 8] = [AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false)];

// تفويض فك تشفير الصور للنواة 3 والنواة 4 في الخلفية
static DECODE_STATUS: AtomicI32 = AtomicI32::new(0); // 0=idle, 1=running, 2=ready, -1=error
static DECODE_W: AtomicUsize = AtomicUsize::new(0);
static DECODE_H: AtomicUsize = AtomicUsize::new(0);
static mut DECODE_INPUT: Option<Vec<u8>> = None;
static mut DECODE_OUTPUT: Option<Vec<u32>> = None;

pub fn start_multicore_decode(data: Vec<u8>) {
    unsafe {
        *addr_of_mut!(DECODE_INPUT) = Some(data);
        *addr_of_mut!(DECODE_OUTPUT) = None;
    }
    DECODE_STATUS.store(1, Ordering::SeqCst);

    // البحث ديناميكياً عن أول نواة SMP حرة وجاهزة من Core 3 حتى Core 7
    for c in 3..8 {
        if crate::CORE_READY[c].load(Ordering::Acquire) && !CORE_IS_BUSY[c].load(Ordering::Acquire) {
            if dispatch_job(c, core_decode_dynamic_worker).is_ok() {
                crate::log_info!("SMP", "Dispatching async image decode task to dynamic Core {}", c);
                return;
            }
        }
    }
    crate::log_error!("SMP", "No free SMP worker core (3..7) available for async image decode.");
    DECODE_STATUS.store(-1, Ordering::Release);
}

pub fn poll_multicore_decode() -> (i32, usize, usize, Option<Vec<u32>>) {
    let status = DECODE_STATUS.load(Ordering::Acquire);
    if status == 2 {
        let w = DECODE_W.load(Ordering::Acquire);
        let h = DECODE_H.load(Ordering::Acquire);
        let px = unsafe { (*addr_of_mut!(DECODE_OUTPUT)).take() };
        DECODE_STATUS.store(0, Ordering::Release);
        (1, w, h, px)
    } else if status == -1 {
        DECODE_STATUS.store(0, Ordering::Release);
        (-1, 0, 0, None)
    } else {
        (0, 0, 0, None)
    }
}

pub struct SpinLockGuard(usize);
impl Drop for SpinLockGuard { fn drop(&mut self) { JOB_LOCKS[self.0].store(false, Ordering::Release); } }

pub fn lock_job(core_id: usize) -> SpinLockGuard {
    while JOB_LOCKS[core_id].compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() { core::hint::spin_loop(); }
    SpinLockGuard(core_id)
}

pub fn dispatch_job(core_id: usize, job: fn()) -> Result<(), &'static str> {
    if core_id == 0 || core_id >= 8 { return Err("Invalid core ID"); }
    let _guard = lock_job(core_id);
    unsafe {
        let slot = &mut CORE_JOBS[core_id];
        if slot.state == JobState::Running || slot.state == JobState::Submitted { return Err("Target core busy"); }
        slot.task_fn = Some(job); slot.state = JobState::Submitted;
    }
    Ok(())
}

pub fn request_elf_execution(filename: &str, argument: &str) -> Result<usize, &'static str> {
    let order: [usize; 7] = if filename == "user_app.elf" && argument.is_empty() {
        [2, 3, 4, 5, 6, 7, 1]
    } else {
        [3, 4, 5, 6, 7, 1, 2]
    };
    let mut selected_core = None;
    for c in order {
        if crate::CORE_READY[c].load(Ordering::Acquire) && !CORE_IS_BUSY[c].load(Ordering::Acquire) {
            selected_core = Some(c);
            break;
        }
    }
    let core_id = match selected_core {
        Some(c) => c,
        None => return Err("No free SMP cores available for ELF execution"),
    };
    unsafe {
        core::ptr::write_bytes(addr_of_mut!(ELF_TARGET_FILES[core_id]) as *mut u8, 0, 128);
        core::ptr::write_bytes(addr_of_mut!(ELF_TARGET_ARGS[core_id]) as *mut u8, 0, 128);
        let fb = filename.as_bytes(); let flen = core::cmp::min(fb.len(), 127);
        core::ptr::copy_nonoverlapping(fb.as_ptr(), addr_of_mut!(ELF_TARGET_FILES[core_id]) as *mut u8, flen);
        let ab = argument.as_bytes(); let alen = core::cmp::min(ab.len(), 127);
        core::ptr::copy_nonoverlapping(ab.as_ptr(), addr_of_mut!(ELF_TARGET_ARGS[core_id]) as *mut u8, alen);
    }
    clear_core_stdout(core_id);
    CORE_IS_BUSY[core_id].store(true, Ordering::SeqCst);
    CORE_ELF_SPAWN_REQ[core_id].store(true, Ordering::SeqCst);
    ELF_ACTIVE_RUNNING.store(true, Ordering::SeqCst);
    Ok(core_id)
}

static STDOUT_CAP_LOCK: AtomicBool = AtomicBool::new(false);
static mut CORE_STDOUT_CAP: [Vec<u8>; 8] = [const { Vec::new() }; 8];

pub fn clear_core_stdout(core_id: usize) {
    if core_id >= 8 { return; }
    while STDOUT_CAP_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
    unsafe { CORE_STDOUT_CAP[core_id].clear(); }
    STDOUT_CAP_LOCK.store(false, Ordering::Release);
}

pub fn append_core_stdout(core_id: usize, data: &[u8]) {
    if core_id >= 8 || data.is_empty() { return; }
    while STDOUT_CAP_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
    unsafe {
        let buf = &mut CORE_STDOUT_CAP[core_id];
        if buf.len() < 65536 {
            let take = (65536 - buf.len()).min(data.len());
            buf.extend_from_slice(&data[..take]);
        }
    }
    STDOUT_CAP_LOCK.store(false, Ordering::Release);
}

pub fn take_core_stdout(core_id: usize) -> Vec<u8> {
    if core_id >= 8 { return Vec::new(); }
    while STDOUT_CAP_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
    let out = unsafe { core::mem::take(&mut CORE_STDOUT_CAP[core_id]) };
    STDOUT_CAP_LOCK.store(false, Ordering::Release);
    out
}

fn elf_runner_worker(core_id: usize) {
    let mut file_buf = [0u8; 128]; let mut arg_buf = [0u8; 128];
    unsafe {
        core::ptr::copy_nonoverlapping(addr_of_mut!(ELF_TARGET_FILES[core_id]) as *const u8, file_buf.as_mut_ptr(), 128);
        core::ptr::copy_nonoverlapping(addr_of_mut!(ELF_TARGET_ARGS[core_id]) as *const u8, arg_buf.as_mut_ptr(), 128);
    }
    let file_len = file_buf.iter().position(|&b| b == 0).unwrap_or(128);
    let arg_len = arg_buf.iter().position(|&b| b == 0).unwrap_or(128);
    let filename = core::str::from_utf8(&file_buf[..file_len]).unwrap_or("");
    let arg = core::str::from_utf8(&arg_buf[..arg_len]).unwrap_or("");
    let proc_name = arg.split_whitespace().next().filter(|s| !s.is_empty()).unwrap_or(filename);

    crate::log_info!("PROC", "Core {} -> Starting isolated ELF '{}'...", core_id, proc_name);
    let t_start = crate::arch::x86_64::pit::read_tsc();

    // كاش في الرام لملف user_app.elf لتسريع إطلاق التطبيقات المنفصلة بدون إعادة قراءة 6.8MB من القرص
    static ELF_CACHE_LOCK: AtomicBool = AtomicBool::new(false);
    static mut CACHED_USER_APP_ELF: Option<Vec<u8>> = None;

    let cached_slice: Option<&'static [u8]> = if filename == "user_app.elf" {
        while ELF_CACHE_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        let res = unsafe {
            if (*core::ptr::addr_of!(CACHED_USER_APP_ELF)).is_none() {
                if let Ok(bytes) = crate::fs::vfs_read_bytes(filename) {
                    *addr_of_mut!(CACHED_USER_APP_ELF) = Some(bytes);
                }
            }
            (*core::ptr::addr_of!(CACHED_USER_APP_ELF)).as_ref().map(|v| v.as_slice())
        };
        ELF_CACHE_LOCK.store(false, Ordering::Release);
        res
    } else {
        None
    };

    if let Some(elf_bytes) = cached_slice {
        register_proc((core_id + 1) as u32, core_id as u32, proc_name, elf_bytes.len() as u64);
        IS_RUNNING_PROGRAM.store(true, Ordering::SeqCst);
        if let Err(err) = crate::fs::elf::load_and_run_elf(elf_bytes, arg) {
            crate::log_error!("PROC", "Core {} -> ELF load error: {}", core_id, err);
        }
        IS_RUNNING_PROGRAM.store(false, Ordering::SeqCst);
    } else {
        match crate::fs::vfs_read_bytes(filename) {
            Ok(elf_bytes) => {
                register_proc((core_id + 1) as u32, core_id as u32, proc_name, elf_bytes.len() as u64);
                IS_RUNNING_PROGRAM.store(true, Ordering::SeqCst);
                if let Err(err) = crate::fs::elf::load_and_run_elf(&elf_bytes, arg) {
                    crate::log_error!("PROC", "Core {} -> ELF load error: {}", core_id, err);
                }
                IS_RUNNING_PROGRAM.store(false, Ordering::SeqCst);
            }
            Err(e) => {
                crate::log_error!("PROC", "Core {} -> VFS failed to read '{}': {}", core_id, filename, e);
            }
        }
    }

    // عزل الأعطال: تنظيف نافذة وذاكرة التطبيق المنهار فقط بدون المساس بسطح المكتب
    destroy_ext_windows_for_core(core_id as u32);
    unregister_proc(core_id as u32);
    CORE_IS_BUSY[core_id].store(false, Ordering::Release);
    let mut any_busy = false;
    for c in 1..8 { if CORE_IS_BUSY[c].load(Ordering::Relaxed) { any_busy = true; break; } }
    ELF_ACTIVE_RUNNING.store(any_busy, Ordering::SeqCst);
    let t_end = crate::arch::x86_64::pit::read_tsc();
    crate::profiler::trace_event(crate::profiler::EventType::TaskExec, core_id as u64, t_start, t_end.saturating_sub(t_start));
}

pub fn force_exit_user_process() -> ! {
    unsafe {
        let core_id = crate::profiler::get_core_id();
        let state_ptr = core::ptr::addr_of_mut!(crate::arch::x86_64::syscall::CORE_SYSCALL_STATES[core_id]);
        let spawn_rsp = (*state_ptr).kernel_spawn_rsp;
        // Restore GS_BASE / KERNEL_GS_BASE deterministically (safe from both #PF/#GP exceptions and syscalls)
        crate::arch::x86_64::syscall::wrmsr(0xC0000101, 0);
        crate::arch::x86_64::syscall::wrmsr(0xC0000102, state_ptr as u64);
        core::arch::asm!(
            "mov rsp, {0}",
            "pop r15", "pop r14", "pop r13", "pop r12", "pop rbx", "pop rbp",
            "ret",
            in(reg) spawn_rsp
        );
        loop { core::hint::spin_loop(); }
    }
}

pub fn core_poll_and_execute(core_id: usize) -> bool {
    if core_id == 0 || core_id >= 8 { return false; }

    if CORE_ELF_SPAWN_REQ[core_id].swap(false, Ordering::SeqCst) {
        crate::log_info!("PROC", "Core {} acknowledged ELF execution request!", core_id);
        elf_runner_worker(core_id);
        return true;
    }

    let mut to_run = None;
    {
        let _guard = lock_job(core_id);
        unsafe {
            let slot = &mut CORE_JOBS[core_id];
            if slot.state == JobState::Submitted {
                to_run = slot.task_fn;
                slot.state = JobState::Running;
            }
        }
    }
    if let Some(func) = to_run {
        let t_start = crate::arch::x86_64::pit::read_tsc();
        func();
        let t_end = crate::arch::x86_64::pit::read_tsc();
        crate::profiler::trace_event(crate::profiler::EventType::TaskExec, core_id as u64, t_start, t_end.saturating_sub(t_start));
        let _guard = lock_job(core_id);
        unsafe {
            CORE_JOBS[core_id].state = JobState::Finished;
            CORE_JOBS[core_id].task_fn = None;
        }
        return true;
    }
    false
}

pub fn init() {
    let scheduler = TaskScheduler { tasks: Vec::new(), current_task_idx: 0, preemptive_enabled: false };
    unsafe { *addr_of_mut!(SCHEDULER) = Some(scheduler); }
}

#[unsafe(no_mangle)]
pub extern "C" fn schedule_preemptive(current_rsp: u64) -> u64 {
    if IS_RUNNING_PROGRAM.load(Ordering::Relaxed) { return current_rsp; }
    unsafe {
        if let Some(sched) = &mut *addr_of_mut!(SCHEDULER) {
            if !sched.preemptive_enabled || sched.tasks.len() <= 1 { return current_rsp; }
            let prev_idx = sched.current_task_idx; sched.tasks[prev_idx].rsp = current_rsp;
            let next_idx = (prev_idx + 1) % sched.tasks.len(); sched.current_task_idx = next_idx;
            return sched.tasks[next_idx].rsp;
        }
    }
    current_rsp
}

pub fn enable_preemption() { unsafe { if let Some(sched) = &mut *addr_of_mut!(SCHEDULER) { sched.preemptive_enabled = true; } } }
pub fn yield_now() {
    unsafe {
        if let Some(sched) = &mut *addr_of_mut!(SCHEDULER) {
            let total = sched.tasks.len(); if total <= 1 { return; }
            let prev_idx = sched.current_task_idx; let next_idx = (prev_idx + 1) % total;
            sched.current_task_idx = next_idx;
            let prev_rsp_ptr = &mut sched.tasks[prev_idx].rsp as *mut u64;
            context::context_switch(prev_rsp_ptr, sched.tasks[next_idx].rsp);
        }
    }
}

fn core_decode_dynamic_worker() {
    let input = unsafe { (*addr_of_mut!(DECODE_INPUT)).take() };
    if let Some(bytes) = input {
        let core_id = crate::profiler::get_core_id();
        crate::log_info!("SMP", "Core {} executing image decode ({} bytes)...", core_id, bytes.len());
        match crate::fs::image::decode_image_to_raw(&bytes) {
            Ok((pixels, w, h)) => {
                crate::log_info!("SMP", "Core {} -> Decode successful: {}x{} px", core_id, w, h);
                DECODE_W.store(w, Ordering::Release);
                DECODE_H.store(h, Ordering::Release);
                unsafe { *addr_of_mut!(DECODE_OUTPUT) = Some(pixels); }
                DECODE_STATUS.store(2, Ordering::Release);
            }
            Err(e) => {
                crate::log_error!("SMP", "Core {} -> Decode error: {}", core_id, e);
                DECODE_STATUS.store(-1, Ordering::Release);
            }
        }
    } else {
        DECODE_STATUS.store(-1, Ordering::Release);
    }
}

pub struct IpcPipe {
    pub data: alloc::collections::VecDeque<u8>,
    pub closed: bool,
}

pub struct PipeRegistryLock {
    pub locked: core::sync::atomic::AtomicBool,
}

impl PipeRegistryLock {
    pub const fn new() -> Self { Self { locked: core::sync::atomic::AtomicBool::new(false) } }
    pub fn lock(&self) { while self.locked.compare_exchange_weak(false, true, core::sync::atomic::Ordering::Acquire, core::sync::atomic::Ordering::Relaxed).is_err() { core::hint::spin_loop(); } }
    pub fn unlock(&self) { self.locked.store(false, core::sync::atomic::Ordering::Release); }
}

pub static PIPE_LOCK: PipeRegistryLock = PipeRegistryLock::new();

pub static mut PIPE_REGISTRY: [Option<IpcPipe>; 128] = [const { None }; 128];

pub fn create_pipe() -> Option<(usize, usize)> {
    PIPE_LOCK.lock();
    let mut id = None;
    unsafe {
        for i in 0..128 {
            if PIPE_REGISTRY[i].is_none() {
                PIPE_REGISTRY[i] = Some(IpcPipe { data: alloc::collections::VecDeque::new(), closed: false });
                id = Some(i);
                break;
            }
        }
    }
    PIPE_LOCK.unlock();
    id.map(|i| (i, i))
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct ProcTelemetry {
    pub pid: u32,
    pub core_id: u32,
    pub state: u32, // 0=Free, 1=Running, 2=Active, 3=Exited
    pub _pad: u32,
    pub mem_bytes: u64,
    pub cpu_cycles: u64,
    pub syscalls: u64,
    pub start_tsc: u64,
    pub name: [u8; 24],
}

impl ProcTelemetry {
    pub const fn empty() -> Self {
        Self {
            pid: 0, core_id: 0, state: 0, _pad: 0,
            mem_bytes: 0, cpu_cycles: 0, syscalls: 0, start_tsc: 0,
            name: [0; 24],
        }
    }
}

pub const MAX_PROCS: usize = 16;
static PROC_TEL_LOCK: AtomicBool = AtomicBool::new(false);
static mut PROC_TELEMETRY: [ProcTelemetry; MAX_PROCS] = [ProcTelemetry::empty(); MAX_PROCS];

fn lock_tel() {
    while PROC_TEL_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
}
fn unlock_tel() { PROC_TEL_LOCK.store(false, Ordering::Release); }

pub fn register_proc(pid: u32, core_id: u32, name: &str, init_mem: u64) {
    lock_tel();
    unsafe {
        let idx = (pid as usize).min(MAX_PROCS - 1);
        let mut p = ProcTelemetry::empty();
        p.pid = pid;
        p.core_id = core_id;
        p.state = 1;
        p.mem_bytes = init_mem;
        p.start_tsc = crate::arch::x86_64::pit::read_tsc();
        let nb = name.as_bytes();
        let len = nb.len().min(23);
        p.name[..len].copy_from_slice(&nb[..len]);
        PROC_TELEMETRY[idx] = p;
    }
    unlock_tel();
}

pub fn record_proc_mem(core_id: usize, delta_bytes: u64) {
    lock_tel();
    unsafe {
        for p in PROC_TELEMETRY.iter_mut() {
            if p.state == 1 && p.core_id as usize == core_id {
                p.mem_bytes = p.mem_bytes.saturating_add(delta_bytes);
                break;
            }
        }
    }
    unlock_tel();
}

pub fn record_proc_syscall(core_id: usize, cycles: u64) {
    unsafe {
        for p in PROC_TELEMETRY.iter_mut() {
            if p.state == 1 && p.core_id as usize == core_id {
                p.syscalls = p.syscalls.wrapping_add(1);
                p.cpu_cycles = p.cpu_cycles.wrapping_add(cycles);
                break;
            }
        }
    }
}

pub fn unregister_proc(core_id: u32) {
    lock_tel();
    unsafe {
        for p in PROC_TELEMETRY.iter_mut() {
            if p.state == 1 && p.core_id == core_id && p.pid >= 3 {
                p.state = 0;
            }
        }
    }
    unlock_tel();
}

pub fn snapshot_procs(out: &mut [ProcTelemetry; MAX_PROCS]) -> usize {
    lock_tel();
    let mut count = 0usize;
    unsafe {
        PROC_TELEMETRY[0].mem_bytes = crate::mm::heap::HEAP_ALLOCATOR.used() as u64;
        PROC_TELEMETRY[0].cpu_cycles = crate::arch::x86_64::pit::read_tsc();
        for p in PROC_TELEMETRY.iter() {
            if p.state != 0 {
                out[count] = *p;
                count += 1;
            }
        }
    }
    unlock_tel();
    count
}

// ============================================================================
// Milestone 12: User Process Crash Inspector Record
// ============================================================================

#[repr(C)]
#[derive(Copy, Clone)]
pub struct CrashReport {
    pub valid: u32,
    pub fault_kind: u32, // 1=#DE, 2=#UD, 3=#GP, 4=#PF
    pub core_id: u32,
    pub error_code: u32,
    pub rip: u64,
    pub cr2: u64,
    pub app_name: [u8; 32],
    pub spawn_args: [u8; 96],
}

impl CrashReport {
    pub const fn empty() -> Self {
        Self {
            valid: 0,
            fault_kind: 0,
            core_id: 0,
            error_code: 0,
            rip: 0,
            cr2: 0,
            app_name: [0; 32],
            spawn_args: [0; 96],
        }
    }
}

static CRASH_LOCK: AtomicBool = AtomicBool::new(false);
static mut LAST_CRASH: CrashReport = CrashReport::empty();

pub fn record_user_crash(core_id: usize, fault_kind: u32, rip: u64, cr2: u64, error_code: u64) {
    while CRASH_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
    unsafe {
        let mut r = CrashReport::empty();
        r.valid = 1;
        r.fault_kind = fault_kind;
        r.core_id = core_id as u32;
        r.error_code = error_code as u32;
        r.rip = rip;
        r.cr2 = cr2;
        if core_id < 8 {
            let args_src = &ELF_TARGET_ARGS[core_id];
            r.spawn_args.copy_from_slice(&args_src[..96]);
            let alen = args_src.iter().position(|&b| b == 0).unwrap_or(96);
            let args_str = core::str::from_utf8(&args_src[..alen]).unwrap_or("app");
            let name = args_str.split_whitespace().next().unwrap_or("app");
            let nb = name.as_bytes();
            r.app_name[..nb.len().min(31)].copy_from_slice(&nb[..nb.len().min(31)]);
        }
        LAST_CRASH = r;
    }
    CRASH_LOCK.store(false, Ordering::Release);
}

pub fn take_last_crash() -> Option<CrashReport> {
    while CRASH_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
    let res = unsafe {
        if LAST_CRASH.valid != 0 {
            let copy = LAST_CRASH;
            LAST_CRASH.valid = 0;
            Some(copy)
        } else {
            None
        }
    };
    CRASH_LOCK.store(false, Ordering::Release);
    res
}

pub const MAX_EXT_WINDOWS: usize = 8;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct ExtWindowDesc {
    pub id: u32,
    pub core_id: u32,
    pub width: u32,
    pub height: u32,
    pub generation: u32,
    pub file_req: u32,
    pub title: [u8; 40],
}

impl Default for ExtWindowDesc {
    fn default() -> Self {
        Self {
            id: 0,
            core_id: 0,
            width: 0,
            height: 0,
            generation: 0,
            file_req: 0,
            title: [0; 40],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct ExtWindowEvent {
    pub kind: u32, // 1=Mouse, 2=Scroll, 3=Key, 4=Char, 5=Resize, 9=Close
    pub a: i32,
    pub b: i32,
    pub c: i32,
}

pub struct ExtWindowSlot {
    pub id: u32,
    pub core_id: u32,
    pub width: u32,
    pub height: u32,
    pub generation: u32,
    pub file_req: u32,
    pub closing: bool,
    pub picked_file: [u8; 128],
    pub picked_ready: bool,
    pub title: [u8; 40],
    pub pixels: Vec<u32>,
    pub events: alloc::collections::VecDeque<ExtWindowEvent>,
}

static EXT_WIN_LOCK: AtomicBool = AtomicBool::new(false);
static NEXT_EXT_WIN_ID: AtomicUsize = AtomicUsize::new(1);
static mut EXT_WINDOWS: [Option<ExtWindowSlot>; MAX_EXT_WINDOWS] = [const { None }; MAX_EXT_WINDOWS];

fn lock_ew() { while EXT_WIN_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() { core::hint::spin_loop(); } }
fn unlock_ew() { EXT_WIN_LOCK.store(false, Ordering::Release); }

pub fn create_ext_window(core_id: u32, title: &str, w: u32, h: u32) -> Option<u32> {
    let w = w.clamp(160, 2560);
    let h = h.clamp(120, 1440);
    lock_ew();
    let mut created = None;
    unsafe {
        for slot in EXT_WINDOWS.iter_mut() {
            if slot.is_none() {
                let id = NEXT_EXT_WIN_ID.fetch_add(1, Ordering::Relaxed) as u32;
                let mut tbuf = [0u8; 40];
                let tb = title.as_bytes();
                tbuf[..tb.len().min(39)].copy_from_slice(&tb[..tb.len().min(39)]);
                *slot = Some(ExtWindowSlot {
                    id, core_id, width: w, height: h, generation: 1, file_req: 0, closing: false,
                    picked_file: [0; 128], picked_ready: false, title: tbuf,
                    pixels: alloc::vec![0xFF0F172A; (w as usize) * (h as usize)],
                    events: alloc::collections::VecDeque::with_capacity(64),
                });
                created = Some(id);
                break;
            }
        }
    }
    unlock_ew();
    created
}

pub fn commit_ext_window(win_id: u32, w: u32, h: u32, src: &[u32]) -> bool {
    let w = w.clamp(160, 2560);
    let h = h.clamp(120, 1440);
    let needed = (w as usize) * (h as usize);
    if src.len() < needed { return false; }
    lock_ew();
    let mut ok = false;
    unsafe {
        for slot in EXT_WINDOWS.iter_mut().flatten() {
            if slot.id == win_id {
                if slot.pixels.len() != needed { slot.pixels.resize(needed, 0xFF0F172A); }
                slot.width = w; slot.height = h;
                slot.pixels[..needed].copy_from_slice(&src[..needed]);
                slot.generation = slot.generation.wrapping_add(1);
                ok = true;
                break;
            }
        }
    }
    unlock_ew();
    ok
}

pub fn poll_ext_event(win_id: u32) -> Option<ExtWindowEvent> {
    lock_ew();
    let mut ev = None;
    unsafe { for s in EXT_WINDOWS.iter_mut().flatten() { if s.id == win_id { ev = s.events.pop_front(); break; } } }
    unlock_ew();
    ev
}

pub fn push_ext_event(win_id: u32, ev: ExtWindowEvent) -> bool {
    lock_ew();
    let mut ok = false;
    unsafe {
        for s in EXT_WINDOWS.iter_mut().flatten() {
            if s.id == win_id {
                if ev.kind == 9 {
                    s.closing = true;
                }
                if s.events.len() < 64 { s.events.push_back(ev); }
                ok = true; break;
            }
        }
    }
    unlock_ew();
    ok
}

pub fn destroy_ext_window(win_id: u32) {
    lock_ew();
    unsafe { for slot in EXT_WINDOWS.iter_mut() { if slot.as_ref().map(|s| s.id == win_id).unwrap_or(false) { *slot = None; break; } } }
    unlock_ew();
}

pub fn destroy_ext_windows_for_core(core_id: u32) {
    lock_ew();
    unsafe { for slot in EXT_WINDOWS.iter_mut() { if slot.as_ref().map(|s| s.core_id == core_id).unwrap_or(false) { *slot = None; } } }
    unlock_ew();
}

pub fn terminate_proc_by_pid(pid: u32) -> bool {
    if pid < 3 {
        return false; // Protect kernel & desktop core services
    }
    let mut target_core: Option<u32> = None;
    lock_tel();
    unsafe {
        for p in PROC_TELEMETRY.iter() {
            if p.state == 1 && p.pid == pid {
                target_core = Some(p.core_id);
                break;
            }
        }
    }
    unlock_tel();

    let Some(core_id) = target_core else { return false; };
    if core_id == 2 {
        return false; // Protect primary desktop compositor
    }

    let mut signaled = false;
    lock_ew();
    unsafe {
        for slot in EXT_WINDOWS.iter_mut().flatten() {
            if slot.core_id == core_id {
                slot.closing = true;
                if slot.events.len() < 64 {
                    slot.events.push_back(ExtWindowEvent { kind: 9, a: 0, b: 0, c: 0 });
                }
                signaled = true;
            }
        }
    }
    unlock_ew();
    signaled
}

pub fn list_ext_windows(out: &mut [ExtWindowDesc; MAX_EXT_WINDOWS]) -> usize {
    lock_ew();
    let mut n = 0;
    unsafe {
        for s in EXT_WINDOWS.iter_mut().flatten() {
            if s.closing { continue; }
            out[n] = ExtWindowDesc { id: s.id, core_id: s.core_id, width: s.width, height: s.height, generation: s.generation, file_req: s.file_req, title: s.title };
            if s.file_req != 0 { s.file_req = 0; }
            n += 1;
        }
    }
    unlock_ew();
    n
}

pub fn read_ext_surface(win_id: u32, dst: &mut [u32]) -> usize {
    lock_ew();
    let mut copied = 0;
    unsafe {
        for s in EXT_WINDOWS.iter().flatten() {
            if s.id == win_id {
                copied = s.pixels.len().min(dst.len());
                dst[..copied].copy_from_slice(&s.pixels[..copied]);
                break;
            }
        }
    }
    unlock_ew();
    copied
}

pub fn ext_file_portal(cmd: u32, win_id: u32, buf: &mut [u8]) -> usize {
    lock_ew();
    let mut ret = 0usize;
    unsafe {
        for s in EXT_WINDOWS.iter_mut().flatten() {
            if s.id == win_id {
                match cmd {
                    0 => {
                        s.file_req |= 1;
                        let len = buf.len().min(127);
                        s.picked_file.fill(0);
                        if len > 0 {
                            s.picked_file[..len].copy_from_slice(&buf[..len]);
                        }
                        ret = 1;
                    }
                    1 => {
                        let len = buf.len().min(127);
                        s.picked_file.fill(0);
                        s.picked_file[..len].copy_from_slice(&buf[..len]);
                        s.picked_ready = true;
                        ret = len;
                    }
                    2 => {
                        if s.picked_ready {
                            let len = s.picked_file.iter().position(|&b| b == 0).unwrap_or(127).min(buf.len());
                            buf[..len].copy_from_slice(&s.picked_file[..len]);
                            s.picked_ready = false;
                            ret = len;
                        }
                    }
                    3 => { s.file_req |= 4; ret = 1; } // Toggle True Fullscreen in Compositor
                    4 => {
                        let len = s.picked_file.iter().position(|&b| b == 0).unwrap_or(127).min(buf.len());
                        if len > 0 {
                            buf[..len].copy_from_slice(&s.picked_file[..len]);
                        }
                        ret = len;
                    }
                    5 => { s.file_req |= 8; ret = 1; } // Toggle Always-on-Top
                    _ => {}
                }
                break;
            }
        }
    }
    unlock_ew();
    ret
}
