pub mod context;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use crate::arch::x86_64::interrupts::IS_RUNNING_PROGRAM;
use crate::log_info;
use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicBool, Ordering};

pub const TASK_STACK_SIZE: usize = 16 * 1024;

pub struct FileDescriptor {
    #[allow(dead_code)]
    pub path: String,
    pub data: Vec<u8>,
    pub offset: usize,
}

#[repr(C)]
pub struct Task {
    pub id: u64,
    pub rsp: u64,
    pub name: &'static str,
    pub counter: u64,
    stack: Box<[u8; TASK_STACK_SIZE]>,
    pub mmap_bump: u64,
    pub fd_table: [Option<FileDescriptor>; 32],
}

impl Task {
    pub fn new(id: u64, name: &'static str, entry: extern "C" fn()) -> Self {
        let stack = Box::new([0u8; TASK_STACK_SIZE]);
        let stack_top = stack.as_ptr() as u64 + TASK_STACK_SIZE as u64;

        let mut current_sp = stack_top;

        unsafe {
            current_sp &= !0xF;
            current_sp -= 8; *(current_sp as *mut u64) = 0x10;
            current_sp -= 8; *(current_sp as *mut u64) = stack_top - 16;
            current_sp -= 8; *(current_sp as *mut u64) = 0x202;
            current_sp -= 8; *(current_sp as *mut u64) = 0x08;
            current_sp -= 8; *(current_sp as *mut u64) = entry as usize as u64;
            for _ in 0..15 { current_sp -= 8; *(current_sp as *mut u64) = 0; }
        }

        Self {
            id,
            rsp: current_sp,
            name,
            counter: 0,
            stack,
            mmap_bump: 0x0000_0001_0000_0000,
            fd_table: Default::default(),
        }
    }
}

pub struct TaskScheduler {
    pub tasks: Vec<Task>,
    pub current_task_idx: usize,
    pub preemptive_enabled: bool,
}

pub static mut SCHEDULER: Option<TaskScheduler> = None;

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum JobState {
    Idle, Submitted, Running, Finished, Failed,
}

pub struct PerCoreJob {
    pub task_fn: Option<fn()>,
    pub state: JobState,
    #[allow(dead_code)]
    pub result: u64,
}

static JOB_LOCKS: [AtomicBool; 8] = [
    AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false),
    AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false),
];

static mut CORE_JOBS: [PerCoreJob; 8] = [
    PerCoreJob { task_fn: None, state: JobState::Idle, result: 0 },
    PerCoreJob { task_fn: None, state: JobState::Idle, result: 0 },
    PerCoreJob { task_fn: None, state: JobState::Idle, result: 0 },
    PerCoreJob { task_fn: None, state: JobState::Idle, result: 0 },
    PerCoreJob { task_fn: None, state: JobState::Idle, result: 0 },
    PerCoreJob { task_fn: None, state: JobState::Idle, result: 0 },
    PerCoreJob { task_fn: None, state: JobState::Idle, result: 0 },
    PerCoreJob { task_fn: None, state: JobState::Idle, result: 0 },
];

// قناة تواصل لطلب تشغيل برامج الـ ELF على نواة مخصصة
pub static mut ELF_TARGET_FILE: [u8; 128] = [0; 128];
pub static mut ELF_TARGET_ARG: [u8; 128] = [0; 128];
pub static ELF_SPAWN_REQUEST: AtomicBool = AtomicBool::new(false);
pub static ELF_ACTIVE_RUNNING: AtomicBool = AtomicBool::new(false);

pub struct SpinLockGuard(usize);
impl Drop for SpinLockGuard {
    fn drop(&mut self) {
        JOB_LOCKS[self.0].store(false, Ordering::Release);
    }
}

pub fn lock_job(core_id: usize) -> SpinLockGuard {
    while JOB_LOCKS[core_id].compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
    SpinLockGuard(core_id)
}

pub fn dispatch_job(core_id: usize, job: fn()) -> Result<(), &'static str> {
    if core_id == 0 || core_id >= 8 { return Err("Invalid core ID"); }
    let _guard = lock_job(core_id);
    unsafe {
        let slot = &mut CORE_JOBS[core_id];
        if slot.state == JobState::Running || slot.state == JobState::Submitted { return Err("Target core busy"); }
        slot.task_fn = Some(job);
        slot.state = JobState::Submitted;
    }
    Ok(())
}

pub fn get_job_state(core_id: usize) -> JobState {
    let _guard = lock_job(core_id);
    unsafe { CORE_JOBS[core_id].state }
}

pub fn request_elf_execution(filename: &str, argument: &str) -> Result<(), &'static str> {
    if ELF_ACTIVE_RUNNING.load(Ordering::SeqCst) {
        return Err("Core 2 is already executing another process");
    }
    unsafe {
        core::ptr::write_bytes(addr_of_mut!(ELF_TARGET_FILE) as *mut u8, 0, 128);
        core::ptr::write_bytes(addr_of_mut!(ELF_TARGET_ARG) as *mut u8, 0, 128);

        let fb = filename.as_bytes();
        let flen = core::cmp::min(fb.len(), 127);
        core::ptr::copy_nonoverlapping(fb.as_ptr(), addr_of_mut!(ELF_TARGET_FILE) as *mut u8, flen);

        let ab = argument.as_bytes();
        let alen = core::cmp::min(ab.len(), 127);
        core::ptr::copy_nonoverlapping(ab.as_ptr(), addr_of_mut!(ELF_TARGET_ARG) as *mut u8, alen);
    }

    ELF_SPAWN_REQUEST.store(true, Ordering::SeqCst);
    Ok(())
}

fn elf_runner_worker() {
    let mut file_buf = [0u8; 128];
    let mut arg_buf = [0u8; 128];

    unsafe {
        core::ptr::copy_nonoverlapping(addr_of_mut!(ELF_TARGET_FILE) as *const u8, file_buf.as_mut_ptr(), 128);
        core::ptr::copy_nonoverlapping(addr_of_mut!(ELF_TARGET_ARG) as *const u8, arg_buf.as_mut_ptr(), 128);
    }

    let file_len = file_buf.iter().position(|&b| b == 0).unwrap_or(128);
    let arg_len = arg_buf.iter().position(|&b| b == 0).unwrap_or(128);

    let filename = core::str::from_utf8(&file_buf[..file_len]).unwrap_or("");
    let arg = core::str::from_utf8(&arg_buf[..arg_len]).unwrap_or("");

    crate::log_info!("PROC", "==================================================");
    crate::log_info!("PROC", "Core 2 -> Spawning Userspace Ring 3 Process");
    crate::log_info!("PROC", "Target Binary: '{}' | Argument: '{}'", filename, arg);

    let t_start = crate::arch::x86_64::pit::read_tsc();

    match crate::fs::vfs_read_bytes(filename) {
        Ok(elf_bytes) => {
            crate::log_info!("PROC", "ELF binary loaded ({} bytes). Initializing Page Tables...", elf_bytes.len());
            IS_RUNNING_PROGRAM.store(true, Ordering::SeqCst);

            match crate::fs::elf::load_and_run_elf(&elf_bytes, arg) {
                Ok(()) => {
                    let t_end = crate::arch::x86_64::pit::read_tsc();
                    let cycles = t_end.saturating_sub(t_start);
                    crate::log_info!("PROC", "[SUCCESS] Process exited cleanly (Total Cycles: {})", cycles);
                }
                Err(err) => {
                    crate::log_error!("PROC", "[FAILED] Execution error: {}", err);
                }
            }

            IS_RUNNING_PROGRAM.store(false, Ordering::SeqCst);
        }
        Err(e) => {
            crate::log_error!("PROC", "Failed to read binary from VFS: {}", e);
        }
    }

    ELF_ACTIVE_RUNNING.store(false, Ordering::SeqCst);
    crate::log_info!("PROC", "Core 2 -> Process Lifecycle Terminated. Core back to IDLE.");
    crate::log_info!("PROC", "==================================================");
}

pub fn core_poll_and_execute(core_id: usize) {
    if core_id == 0 || core_id >= 8 { return; }

    // Core 2 هو المسؤول الحصري عن تشغيل برامج الـ Userspace دون حجز النواة الرسومية
    if core_id == 2 {
        if ELF_SPAWN_REQUEST.swap(false, Ordering::SeqCst) {
            ELF_ACTIVE_RUNNING.store(true, Ordering::SeqCst);
            elf_runner_worker();
            return;
        }
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
        func();
        let _guard = lock_job(core_id);
        unsafe {
            CORE_JOBS[core_id].state = JobState::Finished;
            CORE_JOBS[core_id].task_fn = None;
        }
    }
}

pub fn init() {
    let scheduler = TaskScheduler {
        tasks: Vec::new(),
        current_task_idx: 0,
        preemptive_enabled: false,
    };
    unsafe { *addr_of_mut!(SCHEDULER) = Some(scheduler); }
    log_info!("SCHED", "Task Scheduler ready with FD Tables initialized.");
}

pub fn spawn(id: u64, name: &'static str, entry: extern "C" fn()) {
    unsafe {
        if let Some(sched) = &mut *addr_of_mut!(SCHEDULER) {
            let task = Task::new(id, name, entry);
            sched.tasks.push(task);
            log_info!("SCHED", "Task Spawned: [{}] '{}'", id, name);
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn schedule_preemptive(current_rsp: u64) -> u64 {
    if IS_RUNNING_PROGRAM.load(Ordering::Relaxed) { return current_rsp; }
    unsafe {
        if let Some(sched) = &mut *addr_of_mut!(SCHEDULER) {
            if !sched.preemptive_enabled || sched.tasks.len() <= 1 { return current_rsp; }
            let prev_idx = sched.current_task_idx;
            sched.tasks[prev_idx].rsp = current_rsp;
            let next_idx = (prev_idx + 1) % sched.tasks.len();
            sched.current_task_idx = next_idx;
            return sched.tasks[next_idx].rsp;
        }
    }
    current_rsp
}

pub fn enable_preemption() {
    unsafe {
        if let Some(sched) = &mut *addr_of_mut!(SCHEDULER) {
            sched.preemptive_enabled = true;
        }
    }
}

pub fn yield_now() {
    unsafe {
        if let Some(sched) = &mut *addr_of_mut!(SCHEDULER) {
            let total = sched.tasks.len();
            if total <= 1 { return; }
            let prev_idx = sched.current_task_idx;
            let next_idx = (prev_idx + 1) % total;
            sched.current_task_idx = next_idx;
            let prev_rsp_ptr = &mut sched.tasks[prev_idx].rsp as *mut u64;
            let next_rsp = sched.tasks[next_idx].rsp;
            context::context_switch(prev_rsp_ptr, next_rsp);
        }
    }
}
