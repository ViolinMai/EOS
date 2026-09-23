use core::arch::{asm, naked_asm};
use crate::{log_info, log_error};
use crate::arch::x86_64::pit;
use crate::writer::WRITER;
use crate::fs::vfs_read_bytes;
use crate::task::{SCHEDULER, FileDescriptor};
use crate::mm::paging::{read_cr3, invalidate_tlb, PageTable, PAGE_PRESENT, PAGE_USER, PAGE_WRITABLE};
use crate::mm::paging::VMM;
use alloc::string::String;
use core::sync::atomic::{AtomicBool, Ordering};
use core::ptr::addr_of_mut;

pub static mut KERNEL_SAVED_RSP: u64 = 0;
pub static mut USER_SAVED_RSP: u64 = 0;
pub static mut KERNEL_SYSCALL_STACK_TOP: u64 = 0;
pub static ELF_EXIT_REQUESTED: AtomicBool = AtomicBool::new(false);

static mut SYSCALL_STACK: [u8; 32768] = [0; 32768];

#[inline]
unsafe fn rdmsr(msr: u32) -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        asm!("rdmsr", in("ecx") msr, out("eax") lo, out("edx") hi, options(nomem, nostack, preserves_flags));
    }
    ((hi as u64) << 32) | (lo as u64)
}

#[inline]
unsafe fn wrmsr(msr: u32, value: u64) {
    let lo = (value & 0xFFFFFFFF) as u32;
    let hi = (value >> 32) as u32;
    unsafe {
        asm!("wrmsr", in("ecx") msr, in("eax") lo, in("edx") hi, options(nomem, nostack, preserves_flags));
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SyscallFrame {
    pub rax: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub r10: u64,
    pub r8: u64,
    pub r9: u64,
    pub r11: u64,
    pub rcx: u64,
}

#[unsafe(naked)]
extern "C" fn syscall_entry() {
    naked_asm!(
        "mov [{user_sp}], rsp",
        "mov rsp, [{kernel_sp}]",
        
        "push rcx",
        "push r11",
        "push r9",
        "push r8",
        "push r10",
        "push rdx",
        "push rsi",
        "push rdi",
        "push rax",

        "push rbp",
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        "sub rsp, 8",
        "lea rdi, [rsp + 56]",
        "call {handler}",
        "add rsp, 8",

        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",

        "pop rax",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop r10",
        "pop r8",
        "pop r9",
        "pop r11",
        "pop rcx",

        "mov rsp, [{user_sp}]",
        "sysretq",
        user_sp = sym USER_SAVED_RSP,
        kernel_sp = sym KERNEL_SYSCALL_STACK_TOP,
        handler = sym syscall_handler,
    );
}

pub fn init() {
    unsafe {
        let efer = rdmsr(0xC0000080);
        wrmsr(0xC0000080, efer | 1);
        
        let star = (0x0008u64 << 32) | (0x0010u64 << 48);
        wrmsr(0xC0000081, star);
        
        let lstar = syscall_entry as *const () as u64;
        wrmsr(0xC0000082, lstar);
        
        let fmask = 0x200u64;
        wrmsr(0xC0000084, fmask);

        let stack_ptr = addr_of_mut!(SYSCALL_STACK) as *mut u8;
        let stack_top = stack_ptr as u64 + 32768;
        *addr_of_mut!(KERNEL_SYSCALL_STACK_TOP) = stack_top;
    }
    log_info!("SYSCALL", "System Calls (MSR/Sysret) initialized with POSIX foundations.");
}

unsafe fn read_user_string(ptr: *const u8, max_len: usize) -> Option<String> {
    if ptr.is_null() { return None; }
    let mut len = 0;
    while len < max_len {
        let b = unsafe { *ptr.add(len) };
        if b == 0 { break; }
        len += 1;
    }
    let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
    core::str::from_utf8(slice).map(String::from).ok()
}

#[unsafe(no_mangle)]
pub extern "C" fn syscall_handler(frame_ptr: *mut SyscallFrame) {
    let frame = unsafe { &mut *frame_ptr };
    let syscall_num = frame.rax;

    match syscall_num {
        1 => {
            ELF_EXIT_REQUESTED.store(true, Ordering::SeqCst);
            crate::serial_println!("\n[SYSCALL] Process exited cleanly with code {}.", frame.rdi);
            unsafe {
                let saved_sp = KERNEL_SAVED_RSP;
                if saved_sp != 0 {
                    asm!(
                        "mov rsp, {sp}",
                        "pop r15", "pop r14", "pop r13", "pop r12", "pop rbx", "pop rbp",
                        "ret",
                        sp = in(reg) saved_sp,
                        options(noreturn)
                    );
                }
            }
            frame.rax = 0;
        }
        2 => {
            if frame.rdi == 1 || frame.rdi == 2 {
                let count = core::cmp::min(frame.rdx as usize, 4096); // حماية من سكب بايتات ضخمة
                unsafe {
                    let slice = core::slice::from_raw_parts(frame.rsi as *const u8, count);
                    if let Ok(s) = core::str::from_utf8(slice) {
                        crate::serial_print!("{}", s);
                        if let Some(w) = &mut *addr_of_mut!(WRITER) {
                            w.write_str(s, 203, 213, 225);
                        }
                    }
                }
                frame.rax = count as u64;
            } else {
                frame.rax = -1i64 as u64;
            }
        }
        3 => {
            let fd = frame.rdi as usize;
            let buf = frame.rsi as *mut u8;
            let count = frame.rdx as usize;

            unsafe {
                if let Some(sched) = &mut *addr_of_mut!(SCHEDULER) {
                    let task_idx = sched.current_task_idx;
                    let task = &mut sched.tasks[task_idx];

                    if fd < task.fd_table.len() {
                        if let Some(file_desc) = &mut task.fd_table[fd] {
                            let available = file_desc.data.len().saturating_sub(file_desc.offset);
                            let to_read = core::cmp::min(available, count);
                            
                            if to_read > 0 {
                                core::ptr::copy_nonoverlapping(
                                    file_desc.data.as_ptr().add(file_desc.offset),
                                    buf,
                                    to_read
                                );
                                file_desc.offset += to_read;
                                frame.rax = to_read as u64;
                                return;
                            }
                            frame.rax = 0;
                            return;
                        }
                    }
                }
            }
            frame.rax = -1i64 as u64;
        }
        4 => {
            let path_opt = unsafe { read_user_string(frame.rdi as *const u8, 256) };
            if let Some(path) = path_opt {
                match vfs_read_bytes(&path) {
                    Ok(data) => {
                        unsafe {
                            if let Some(sched) = &mut *addr_of_mut!(SCHEDULER) {
                                let task_idx = sched.current_task_idx;
                                let task = &mut sched.tasks[task_idx];

                                for i in 3..task.fd_table.len() {
                                    if task.fd_table[i].is_none() {
                                        task.fd_table[i] = Some(FileDescriptor {
                                            path,
                                            data,
                                            offset: 0,
                                        });
                                        frame.rax = i as u64;
                                        return;
                                    }
                                }
                            }
                        }
                        frame.rax = -2i64 as u64;
                        return;
                    }
                    Err(_) => {
                        frame.rax = -1i64 as u64;
                        return;
                    }
                }
            }
            frame.rax = -1i64 as u64;
        }
        5 => {
            let fd = frame.rdi as usize;
            unsafe {
                if let Some(sched) = &mut *addr_of_mut!(SCHEDULER) {
                    let task_idx = sched.current_task_idx;
                    let task = &mut sched.tasks[task_idx];

                    if fd < task.fd_table.len() && task.fd_table[fd].is_some() {
                        task.fd_table[fd] = None;
                        frame.rax = 0;
                        return;
                    }
                }
            }
            frame.rax = -1i64 as u64;
        }
        8 => {
            let fd = frame.rdi as usize;
            let offset = frame.rsi as i64;
            let whence = frame.rdx as usize;

            unsafe {
                if let Some(sched) = &mut *addr_of_mut!(SCHEDULER) {
                    let task_idx = sched.current_task_idx;
                    let task = &mut sched.tasks[task_idx];

                    if fd < task.fd_table.len() {
                        if let Some(file_desc) = &mut task.fd_table[fd] {
                            let new_offset = match whence {
                                0 => offset,
                                1 => (file_desc.offset as i64).saturating_add(offset),
                                2 => (file_desc.data.len() as i64).saturating_add(offset),
                                _ => -1,
                            };

                            if new_offset >= 0 && new_offset <= file_desc.data.len() as i64 {
                                file_desc.offset = new_offset as usize;
                                frame.rax = new_offset as u64;
                                return;
                            }
                        }
                    }
                }
            }
            frame.rax = -1i64 as u64;
        }
        9 => {
            let size = frame.rdi;
            if size == 0 { frame.rax = 0; return; }
            
            let pages_needed = (size + 4095) / 4096;
            
            unsafe {
                let vmm = (&*addr_of_mut!(VMM)).as_ref().unwrap();
                let frame_alloc = (*addr_of_mut!(crate::mm::frame::FRAME_ALLOCATOR)).as_mut().unwrap();
                
                if let Some(sched) = &mut *addr_of_mut!(SCHEDULER) {
                    let task_idx = sched.current_task_idx;
                    let task = &mut sched.tasks[task_idx];
                    
                    let start_vaddr = task.mmap_bump;
                    let user_flags = PAGE_PRESENT | PAGE_WRITABLE | PAGE_USER;
                    let pml4_phys = read_cr3() & 0x000F_FFFF_FFFF_F000;
                    let pml4 = &mut *((pml4_phys + vmm.hhdm_offset) as *mut PageTable);

                    let mut current_vaddr = start_vaddr;
                    for _ in 0..pages_needed {
                        let p4_idx = ((current_vaddr >> 39) & 0x1FF) as usize;
                        let p3_idx = ((current_vaddr >> 30) & 0x1FF) as usize;
                        let p2_idx = ((current_vaddr >> 21) & 0x1FF) as usize;
                        let p1_idx = ((current_vaddr >> 12) & 0x1FF) as usize;

                        if !pml4.entries[p4_idx].is_present() {
                            let pdpt_phys = frame_alloc.allocate_frame().unwrap();
                            let pdpt = &mut *((pdpt_phys + vmm.hhdm_offset) as *mut PageTable);
                            core::ptr::write_bytes(pdpt as *mut PageTable as *mut u8, 0, 4096);
                            pml4.entries[p4_idx].set(pdpt_phys, user_flags);
                        }
                        let pdpt_phys = pml4.entries[p4_idx].physical_address();
                        let pdpt = &mut *((pdpt_phys + vmm.hhdm_offset) as *mut PageTable);

                        if !pdpt.entries[p3_idx].is_present() {
                            let pd_phys = frame_alloc.allocate_frame().unwrap();
                            let pd = &mut *((pd_phys + vmm.hhdm_offset) as *mut PageTable);
                            core::ptr::write_bytes(pd as *mut PageTable as *mut u8, 0, 4096);
                            pdpt.entries[p3_idx].set(pd_phys, user_flags);
                        }
                        let pd_phys = pdpt.entries[p3_idx].physical_address();
                        let pd = &mut *((pd_phys + vmm.hhdm_offset) as *mut PageTable);

                        if !pd.entries[p2_idx].is_present() {
                            let pt_phys = frame_alloc.allocate_frame().unwrap();
                            let pt = &mut *((pt_phys + vmm.hhdm_offset) as *mut PageTable);
                            core::ptr::write_bytes(pt as *mut PageTable as *mut u8, 0, 4096);
                            pd.entries[p2_idx].set(pt_phys, user_flags);
                        }
                        let pt_phys = pd.entries[p2_idx].physical_address();
                        let pt = &mut *((pt_phys + vmm.hhdm_offset) as *mut PageTable);

                        if !pt.entries[p1_idx].is_present() {
                            let frame_phys = frame_alloc.allocate_frame().unwrap();
                            let frame_ptr = (frame_phys + vmm.hhdm_offset) as *mut u8;
                            core::ptr::write_bytes(frame_ptr, 0, 4096);
                            pt.entries[p1_idx].set(frame_phys, user_flags);
                            invalidate_tlb(current_vaddr);
                        }
                        
                        current_vaddr += 4096;
                    }

                    task.mmap_bump = current_vaddr;
                    frame.rax = start_vaddr;
                    return;
                }
            }
            frame.rax = 0;
        }
        100 => {
            pit::sleep_ms(frame.rdi);
            frame.rax = 0;
        }
        101 => {
            unsafe {
                if let Some(writer) = &mut *addr_of_mut!(WRITER) {
                    writer.clear(15, 23, 42);
                }
            }
            frame.rax = 0;
        }
        102 => {
            unsafe {
                if let Some(writer) = &mut *addr_of_mut!(WRITER) {
                    let ptr = frame.rdi as *const u32;
                    let x = frame.rsi as usize;
                    let y = frame.rdx as usize;
                    let w = frame.r10 as usize;
                    let h = frame.r8 as usize;
                    if !ptr.is_null() && w > 0 && h > 0 {
                        // قص الصورة تلقائياً لعدم تجاوز مساحة الشاشة
                        let max_w = if x < writer.width { writer.width - x } else { 0 };
                        let max_h = if y < writer.height { writer.height - y } else { 0 };
                        let draw_w = core::cmp::min(w, max_w);
                        let draw_h = core::cmp::min(h, max_h);

                        if draw_w > 0 && draw_h > 0 {
                            let slice = core::slice::from_raw_parts(ptr, w * h);
                            writer.blit_buffer_alpha(slice, x, y, draw_w, draw_h);
                        }
                    }
                }
            }
            frame.rax = 0;
        }
        228 => {
            let tp = frame.rsi as *mut u64;
            if !tp.is_null() {
                let ticks = pit::get_ticks();
                let secs = ticks / 100;
                let nsecs = (ticks % 100) * 10_000_000;
                unsafe {
                    *tp = secs;
                    *tp.add(1) = nsecs;
                }
                frame.rax = 0;
            } else {
                frame.rax = -1i64 as u64;
            }
        }
        _ => {
            log_error!("SYSCALL", "Unknown syscall number: {}", syscall_num);
            frame.rax = -1i64 as u64;
        }
    }
}
