pub mod context;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use crate::arch::x86_64::interrupts::IS_RUNNING_PROGRAM;
use crate::log_info;
use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicBool, Ordering};

pub const TASK_STACK_SIZE: usize = 16 * 1024;

#[derive(Clone, Debug)]
pub enum FileSource {
    Memory(Vec<u8>),
    AtaDisk { drive: u8, first_cluster: u32, size: usize },
    Directory(Vec<crate::fs::FsItem>, usize),
}

#[derive(Clone)]
pub struct FileDescriptor { pub path: String, pub source: FileSource, pub offset: usize }

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

pub static mut ELF_TARGET_FILE: [u8; 128] = [0; 128];
pub static mut ELF_TARGET_ARG: [u8; 128] = [0; 128];
pub static ELF_ACTIVE_RUNNING: AtomicBool = AtomicBool::new(false);
pub static CORE_ELF_SPAWN_REQ: [AtomicBool; 8] = [AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false)];
pub static CORE_IS_BUSY: [AtomicBool; 8] = [AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false)];

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
    let mut selected_core = None;
    for c in 2..8 { if !CORE_IS_BUSY[c].load(Ordering::Acquire) { selected_core = Some(c); break; } }
    let core_id = selected_core.ok_or("All Cores (2-7) are currently busy!")?;
    unsafe {
        core::ptr::write_bytes(addr_of_mut!(ELF_TARGET_FILE) as *mut u8, 0, 128); core::ptr::write_bytes(addr_of_mut!(ELF_TARGET_ARG) as *mut u8, 0, 128);
        let fb = filename.as_bytes(); let flen = core::cmp::min(fb.len(), 127); core::ptr::copy_nonoverlapping(fb.as_ptr(), addr_of_mut!(ELF_TARGET_FILE) as *mut u8, flen);
        let ab = argument.as_bytes(); let alen = core::cmp::min(ab.len(), 127); core::ptr::copy_nonoverlapping(ab.as_ptr(), addr_of_mut!(ELF_TARGET_ARG) as *mut u8, alen);
    }
    CORE_IS_BUSY[core_id].store(true, Ordering::SeqCst); CORE_ELF_SPAWN_REQ[core_id].store(true, Ordering::SeqCst);
    ELF_ACTIVE_RUNNING.store(true, Ordering::SeqCst); Ok(core_id)
}

fn elf_runner_worker(core_id: usize) {
    let mut file_buf = [0u8; 128]; let mut arg_buf = [0u8; 128];
    unsafe { core::ptr::copy_nonoverlapping(addr_of_mut!(ELF_TARGET_FILE) as *const u8, file_buf.as_mut_ptr(), 128); core::ptr::copy_nonoverlapping(addr_of_mut!(ELF_TARGET_ARG) as *const u8, arg_buf.as_mut_ptr(), 128); }
    let file_len = file_buf.iter().position(|&b| b == 0).unwrap_or(128); let arg_len = arg_buf.iter().position(|&b| b == 0).unwrap_or(128);
    let filename = core::str::from_utf8(&file_buf[..file_len]).unwrap_or(""); let arg = core::str::from_utf8(&arg_buf[..arg_len]).unwrap_or("");
    crate::log_info!("PROC", "Core {} -> Loading & Isolating Userspace App", core_id);
    let t_start = crate::arch::x86_64::pit::read_tsc();

    match crate::fs::vfs_read_bytes(filename) {
        Ok(elf_bytes) => {
            IS_RUNNING_PROGRAM.store(true, Ordering::SeqCst);
            let _ = crate::fs::elf::load_and_run_elf(&elf_bytes, arg);
            IS_RUNNING_PROGRAM.store(false, Ordering::SeqCst);
        }
        Err(e) => { crate::log_error!("PROC", "Failed to read binary from VFS: {}", e); }
    }

    CORE_IS_BUSY[core_id].store(false, Ordering::Release); crate::arch::x86_64::syscall::OVERLAY_ACTIVE.store(false, Ordering::Release);
    let mut any_busy = false;
    for c in 2..8 { if CORE_IS_BUSY[c].load(Ordering::Relaxed) { any_busy = true; break; } }
    ELF_ACTIVE_RUNNING.store(any_busy, Ordering::SeqCst);
    let t_end = crate::arch::x86_64::pit::read_tsc();
    crate::profiler::trace_event(crate::profiler::EventType::TaskExec, core_id as u64, t_start, t_end.saturating_sub(t_start));
}

pub fn force_exit_user_process() -> ! {
    unsafe {
        let core_id = crate::profiler::get_core_id();
        let state = &crate::arch::x86_64::syscall::CORE_SYSCALL_STATES[core_id];
        let spawn_rsp = state.kernel_spawn_rsp;
        core::arch::asm!("swapgs", "mov rsp, {0}", "pop r15", "pop r14", "pop r13", "pop r12", "pop rbx", "pop rbp", "ret", in(reg) spawn_rsp);
        loop { core::hint::spin_loop(); }
    }
}

pub fn core_poll_and_execute(core_id: usize) {
    if core_id == 0 || core_id >= 8 { return; }
    if core_id >= 2 && CORE_ELF_SPAWN_REQ[core_id].swap(false, Ordering::SeqCst) { elf_runner_worker(core_id); return; }
    let mut to_run = None;
    {
        let _guard = lock_job(core_id);
        unsafe { let slot = &mut CORE_JOBS[core_id]; if slot.state == JobState::Submitted { to_run = slot.task_fn; slot.state = JobState::Running; } }
    }
    if let Some(func) = to_run {
        let t_start = crate::arch::x86_64::pit::read_tsc(); func(); let t_end = crate::arch::x86_64::pit::read_tsc();
        crate::profiler::trace_event(crate::profiler::EventType::TaskExec, core_id as u64, t_start, t_end.saturating_sub(t_start));
        let _guard = lock_job(core_id); unsafe { CORE_JOBS[core_id].state = JobState::Finished; CORE_JOBS[core_id].task_fn = None; }
    }
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
