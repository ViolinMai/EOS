pub mod context;

use alloc::boxed::Box;
use alloc::vec::Vec;
use crate::arch::x86_64::interrupts::IS_RUNNING_PROGRAM;
use crate::log_info;
use core::ptr::addr_of_mut;
use core::sync::atomic::Ordering;

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

pub fn init() {
    let scheduler = TaskScheduler {
        tasks: Vec::new(),
        current_task_idx: 0,
        preemptive_enabled: false,
    };

    unsafe {
        *addr_of_mut!(SCHEDULER) = Some(scheduler);
    }
    log_info!("Task Scheduler: Preemptive multi-tasking engine ready.");
}

pub fn spawn(id: u64, name: &'static str, entry: extern "C" fn()) {
    unsafe {
        if let Some(sched) = &mut *addr_of_mut!(SCHEDULER) {
            let task = Task::new(id, name, entry);
            sched.tasks.push(task);
            log_info!("Task Spawned: [{}] '{}'", id, name);
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
            log_info!("Preemptive Scheduling: ACTIVE (Timeslice: 10ms via PIT).");
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
