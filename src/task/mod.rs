pub mod context;

use alloc::boxed::Box;
use alloc::vec::Vec;
use crate::arch::x86_64::interrupts::IS_RUNNING_PROGRAM;
use crate::log_info;
use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicBool, Ordering};

pub const TASK_STACK_SIZE: usize = 16 * 1024; // 16 KB لكل مهمة كيرنل

#[repr(C)]
pub struct Task {
    pub id: u64,
    pub rsp: u64,
    pub name: &'static str,
    pub counter: u64,
    stack: Box<[u8; TASK_STACK_SIZE]>,
}

impl Task {
    pub fn new(id: u64, name: &'static str, entry: extern "C" fn()) -> Self {
        let stack = Box::new([0u8; TASK_STACK_SIZE]);
        let stack_top = stack.as_ptr() as u64 + TASK_STACK_SIZE as u64;

        let mut current_sp = stack_top;

        unsafe {
            current_sp &= !0xF;

            current_sp -= 8;
            *(current_sp as *mut u64) = 0x10;
            current_sp -= 8;
            *(current_sp as *mut u64) = stack_top - 16;
            current_sp -= 8;
            *(current_sp as *mut u64) = 0x202;
            current_sp -= 8;
            *(current_sp as *mut u64) = 0x08;
            current_sp -= 8;
            *(current_sp as *mut u64) = entry as usize as u64;

            for _ in 0..15 {
                current_sp -= 8;
                *(current_sp as *mut u64) = 0;
            }
        }

        Self {
            id,
            rsp: current_sp,
            name,
            counter: 0,
            stack,
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
    Idle,
    Submitted,
    Running,
    Finished,
}

pub struct PerCoreJob {
    pub task_fn: Option<fn()>,
    pub state: JobState,
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
    if core_id == 0 || core_id >= 8 {
        return Err("Invalid target core ID (Must be between 1 and 7)");
    }

    let _guard = lock_job(core_id);
    unsafe {
        let slot = &mut CORE_JOBS[core_id];
        if slot.state == JobState::Running || slot.state == JobState::Submitted {
            return Err("Target core is busy with another job");
        }
        slot.task_fn = Some(job);
        slot.state = JobState::Submitted;
    }
    Ok(())
}

pub fn get_job_state(core_id: usize) -> JobState {
    let _guard = lock_job(core_id);
    unsafe { CORE_JOBS[core_id].state }
}

pub fn core_poll_and_execute(core_id: usize) {
    if core_id == 0 || core_id >= 8 {
        return;
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

    unsafe {
        *addr_of_mut!(SCHEDULER) = Some(scheduler);
    }
    log_info!("SCHED", "Task Scheduler: Multi-Core Job Offloading queues ready.");
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
    if IS_RUNNING_PROGRAM.load(Ordering::Relaxed) {
        return current_rsp;
    }

    unsafe {
        if let Some(sched) = &mut *addr_of_mut!(SCHEDULER) {
            if !sched.preemptive_enabled || sched.tasks.len() <= 1 {
                return current_rsp;
            }

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
            log_info!("SCHED", "Preemptive Scheduling: ACTIVE (Timeslice: 10ms via PIT).");
        }
    }
}

pub fn yield_now() {
    unsafe {
        if let Some(sched) = &mut *addr_of_mut!(SCHEDULER) {
            let total = sched.tasks.len();
            if total <= 1 {
                return;
            }

            let prev_idx = sched.current_task_idx;
            let next_idx = (prev_idx + 1) % total;
            sched.current_task_idx = next_idx;

            let prev_rsp_ptr = &mut sched.tasks[prev_idx].rsp as *mut u64;
            let next_rsp = sched.tasks[next_idx].rsp;

            context::context_switch(prev_rsp_ptr, next_rsp);
        }
    }
}
