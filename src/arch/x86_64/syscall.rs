use core::arch::{asm, naked_asm};
use crate::{log_info, log_error, log_warn, log_debug};
use crate::arch::x86_64::pit;
use crate::fs::vfs_read_bytes;
use crate::task::FileDescriptor;
use crate::mm::paging::{read_cr3, invalidate_tlb, PageTable, PAGE_PRESENT, PAGE_USER, PAGE_WRITABLE};
use crate::mm::paging::VMM;
use alloc::string::String;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use core::ptr::addr_of_mut;

pub static mut KERNEL_SAVED_RSP: u64 = 0;
pub static mut USER_SAVED_RSP: u64 = 0;
pub static mut KERNEL_SYSCALL_STACK_TOP: u64 = 0;
pub static mut KERNEL_SYSCALL_STACKS: [u64; 8] = [0; 8];
pub static ELF_EXIT_REQUESTED: AtomicBool = AtomicBool::new(false);

pub const OVERLAY_MAX_W: usize = 800;
pub const OVERLAY_MAX_H: usize = 600;
pub static OVERLAY_ACTIVE: AtomicBool = AtomicBool::new(false);
pub static OVERLAY_WIDTH: AtomicUsize = AtomicUsize::new(0);
pub static OVERLAY_HEIGHT: AtomicUsize = AtomicUsize::new(0);
pub static mut OVERLAY_PIXELS: [u32; OVERLAY_MAX_W * OVERLAY_MAX_H] = [0; OVERLAY_MAX_W * OVERLAY_MAX_H];

pub struct ProcessState {
    pub mmap_bump: u64,
    pub fd_table: [Option<FileDescriptor>; 32],
}
const EMPTY_FD: Option<FileDescriptor> = None;
pub static mut CORE2_PROCESS: ProcessState = ProcessState {
    mmap_bump: 0x0000_0001_0000_0000,
    fd_table: [EMPTY_FD; 32],
};

pub fn reset_core2_process() {
    unsafe {
        CORE2_PROCESS.mmap_bump = 0x0000_0001_0000_0000;
        for i in 0..32 {
            CORE2_PROCESS.fd_table[i] = None;
        }
    }
}

#[inline]
unsafe fn rdmsr(msr: u32) -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe { asm!("rdmsr", in("ecx") msr, out("eax") lo, out("edx") hi, options(nomem, nostack, preserves_flags)); }
    ((hi as u64) << 32) | (lo as u64)
}

#[inline]
unsafe fn wrmsr(msr: u32, value: u64) {
    let lo = (value & 0xFFFFFFFF) as u32;
    let hi = (value >> 32) as u32;
    unsafe { asm!("wrmsr", in("ecx") msr, in("eax") lo, in("edx") hi, options(nomem, nostack, preserves_flags)); }
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
        "swapgs",
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
        "swapgs",
        "sysretq",
        user_sp = sym USER_SAVED_RSP,
        kernel_sp = sym KERNEL_SYSCALL_STACK_TOP,
        handler = sym syscall_handler,
    );
}

pub fn init_core_syscall(core_id: usize) {
    unsafe {
        let efer = rdmsr(0xC0000080);
        wrmsr(0xC0000080, efer | 1);
        
        let star = (0x0008u64 << 32) | (0x0010u64 << 48);
        wrmsr(0xC0000081, star);
        
        let lstar = syscall_entry as *const () as u64;
        wrmsr(0xC0000082, lstar);
        
        let fmask = 0x200u64;
        wrmsr(0xC0000084, fmask);

        if core_id < 8 && KERNEL_SYSCALL_STACKS[core_id] != 0 {
            KERNEL_SYSCALL_STACK_TOP = KERNEL_SYSCALL_STACKS[core_id];
        }
    }
}

pub fn init() {
    init_core_syscall(0);
    log_info!("SYSCALL", "System Calls initialized and mapped.");
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

fn syscall_name(num: u64) -> &'static str {
    match num {
        0 => "read", 1 => "write", 2 => "open", 3 => "close", 4 => "stat",
        5 => "fstat", 7 => "poll", 8 => "lseek", 9 => "mmap", 10 => "mprotect",
        11 => "munmap", 12 => "brk", 13 => "rt_sigaction", 14 => "rt_sigprocmask",
        16 => "ioctl", 19 => "readv", 20 => "writev", 21 => "access", 22 => "pipe",
        24 => "sched_yield", 28 => "madvise", 39 => "getpid", 41 => "socket",
        60 => "exit", 72 => "fcntl", 79 => "getcwd", 89 => "readlink",
        131 => "sigaltstack", 158 => "arch_prctl", 186 => "gettid", 200 => "tkill",
        202 => "futex", 217 => "getdents64", 218 => "set_tid_address",
        228 => "clock_gettime", 231 => "exit_group", 234 => "tgkill",
        257 => "openat", 262 => "fstatat", 302 => "prlimit64", 318 => "getrandom",
        500 => "sys_sleep", 501 => "sys_clear_screen", 502 => "sys_blit_image_ptr",
        _ => "unknown_sys",
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn syscall_handler(frame_ptr: *mut SyscallFrame) {
    let frame = unsafe { &mut *frame_ptr };
    let syscall_num = frame.rax;
    let name = syscall_name(syscall_num);

    // 💡 محلل استدعاءات متقدم لاستخراج النصوص والتفاصيل المهمة
    let mut extra_info = String::new();
    
    if syscall_num == 2 || syscall_num == 257 || syscall_num == 4 || syscall_num == 21 {
        let ptr = if syscall_num == 257 { frame.rsi } else { frame.rdi } as *const u8;
        if let Some(s) = unsafe { read_user_string(ptr, 128) } {
            extra_info = alloc::format!("path=\"{}\"", s);
        }
    } else if syscall_num == 9 { // mmap
        extra_info = alloc::format!("len={}, prot={:#x}, flags={:#x}, fd={}", frame.rsi, frame.rdx, frame.r10, frame.r8 as i64);
    } else if syscall_num == 1 || syscall_num == 0 { // read / write
        extra_info = alloc::format!("fd={}, len={}", frame.rdi, frame.rdx);
    } else if syscall_num == 3 { // close
        extra_info = alloc::format!("fd={}", frame.rdi);
    }

    log_debug!(
        "MIRROR",
        "Syscall {:>3} [{:<14}] ({:#08x}, {:#08x}) {}",
        syscall_num, name, frame.rdi, frame.rsi, extra_info
    );

    match syscall_num {
        // ===================================================================
        // 🪞 MIRROR LAYER (Linux ABI Compatibility Subset for Rust STD) 🪞
        // ===================================================================
        0 => { // read
            let fd = frame.rdi as usize;
            let buf = frame.rsi as *mut u8;
            let count = frame.rdx as usize;

            if fd == 0 {
                let mut read_count = 0;
                unsafe {
                    while read_count < count {
                        if let Some(c) = crate::arch::x86_64::keyboard::pop_char_from_buffer() {
                            core::ptr::write(buf.add(read_count), c);
                            read_count += 1;
                            if c == b'\n' { break; }
                        } else {
                            crate::task::yield_now();
                        }
                    }
                }
                frame.rax = read_count as u64;
                return;
            }

            unsafe {
                let proc = &mut *addr_of_mut!(CORE2_PROCESS);
                if fd < proc.fd_table.len() {
                    if let Some(file_desc) = &mut proc.fd_table[fd] {
                        let available = file_desc.data.len().saturating_sub(file_desc.offset);
                        let to_read = core::cmp::min(available, count);
                        if to_read > 0 {
                            core::ptr::copy_nonoverlapping(file_desc.data.as_ptr().add(file_desc.offset), buf, to_read);
                            file_desc.offset += to_read;
                        }
                        frame.rax = to_read as u64;
                        return;
                    }
                }
            }
            frame.rax = -9i64 as u64; // EBADF
        }
        1 => { // write
            let fd = frame.rdi;
            if fd == 1 || fd == 2 {
                let count = core::cmp::min(frame.rdx as usize, 4096);
                unsafe {
                    let slice = core::slice::from_raw_parts(frame.rsi as *const u8, count);
                    if let Ok(s) = core::str::from_utf8(slice) {
                        crate::serial_print!("{}", s);
                    }
                }
                frame.rax = count as u64;
            } else {
                frame.rax = -9i64 as u64; // EBADF
            }
        }
        2 | 257 => { // open / openat
            let path_ptr = if syscall_num == 257 { frame.rsi } else { frame.rdi } as *const u8;
            let path_opt = unsafe { read_user_string(path_ptr, 256) };
            if let Some(path) = path_opt {
                match vfs_read_bytes(&path) {
                    Ok(data) => {
                        unsafe {
                            let proc = &mut *addr_of_mut!(CORE2_PROCESS);
                            for i in 3..proc.fd_table.len() {
                                if proc.fd_table[i].is_none() {
                                    proc.fd_table[i] = Some(FileDescriptor { path, data, offset: 0 });
                                    frame.rax = i as u64;
                                    return;
                                }
                            }
                        }
                        frame.rax = -24i64 as u64; // EMFILE
                        return;
                    }
                    Err(_) => {
                        frame.rax = -2i64 as u64; // ENOENT
                        return;
                    }
                }
            }
            frame.rax = -2i64 as u64; // ENOENT
        }
        3 => { // close
            let fd = frame.rdi as usize;
            unsafe {
                let proc = &mut *addr_of_mut!(CORE2_PROCESS);
                if fd < proc.fd_table.len() && proc.fd_table[fd].is_some() {
                    proc.fd_table[fd] = None;
                    frame.rax = 0;
                    return;
                }
            }
            frame.rax = -9i64 as u64; // EBADF
        }
        4 | 5 => { // stat / fstat
            let stat_ptr = frame.rsi as *mut u8;
            unsafe {
                core::ptr::write_bytes(stat_ptr, 0, 144);
                let mode_ptr = stat_ptr.add(24) as *mut u32;
                *mode_ptr = 0x81A4; // S_IFREG | 0644
                
                if syscall_num == 5 {
                    let fd = frame.rdi as usize;
                    let proc = &mut *addr_of_mut!(CORE2_PROCESS);
                    if fd < proc.fd_table.len() {
                        if let Some(file_desc) = &proc.fd_table[fd] {
                            let size_ptr = stat_ptr.add(48) as *mut i64;
                            *size_ptr = file_desc.data.len() as i64;
                        }
                    }
                }
            }
            frame.rax = 0;
        }
        7 => { // poll
            frame.rax = 1;
        }
        8 => { // lseek
            let fd = frame.rdi as usize;
            let offset = frame.rsi as i64;
            let whence = frame.rdx as usize;
            unsafe {
                let proc = &mut *addr_of_mut!(CORE2_PROCESS);
                if fd < proc.fd_table.len() {
                    if let Some(file_desc) = &mut proc.fd_table[fd] {
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
            frame.rax = -22i64 as u64; // EINVAL
        }
        9 => { // mmap
            let size = frame.rsi as usize;
            if size == 0 { frame.rax = 0; return; }
            let pages_needed = (size + 4095) / 4096;
            
            unsafe {
                let vmm = (&*addr_of_mut!(VMM)).as_ref().unwrap();
                let frame_alloc = (*addr_of_mut!(crate::mm::frame::FRAME_ALLOCATOR)).as_mut().unwrap();
                
                let proc = &mut *addr_of_mut!(CORE2_PROCESS);
                let start_vaddr = proc.mmap_bump;
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
                        let f = frame_alloc.allocate_frame().unwrap();
                        core::ptr::write_bytes((f + vmm.hhdm_offset) as *mut u8, 0, 4096);
                        pml4.entries[p4_idx].set(f, user_flags);
                    }
                    let pt4 = &mut *((pml4.entries[p4_idx].physical_address() + vmm.hhdm_offset) as *mut PageTable);

                    if !pt4.entries[p3_idx].is_present() {
                        let f = frame_alloc.allocate_frame().unwrap();
                        core::ptr::write_bytes((f + vmm.hhdm_offset) as *mut u8, 0, 4096);
                        pt4.entries[p3_idx].set(f, user_flags);
                    }
                    let pt3 = &mut *((pt4.entries[p3_idx].physical_address() + vmm.hhdm_offset) as *mut PageTable);

                    if !pt3.entries[p2_idx].is_present() {
                        let f = frame_alloc.allocate_frame().unwrap();
                        core::ptr::write_bytes((f + vmm.hhdm_offset) as *mut u8, 0, 4096);
                        pt3.entries[p2_idx].set(f, user_flags);
                    }
                    let pt2 = &mut *((pt3.entries[p2_idx].physical_address() + vmm.hhdm_offset) as *mut PageTable);

                    if !pt2.entries[p1_idx].is_present() {
                        let frame_phys = frame_alloc.allocate_frame().unwrap();
                        let frame_ptr = (frame_phys + vmm.hhdm_offset) as *mut u8;
                        core::ptr::write_bytes(frame_ptr, 0, 4096);
                        pt2.entries[p1_idx].set(frame_phys, user_flags);
                        invalidate_tlb(current_vaddr);
                    }
                    current_vaddr += 4096;
                }

                proc.mmap_bump = current_vaddr;
                frame.rax = start_vaddr;
            }
        }
        10 | 11 | 12 | 13 | 14 | 28 => { 
            // mprotect, munmap, brk, rt_sigaction, rt_sigprocmask, madvise
            frame.rax = 0;
        }
        16 => { // ioctl
            frame.rax = -25i64 as u64; // ENOTTY
        }
        20 => { // writev
            #[repr(C)]
            struct Iovec { base: *const u8, len: usize }
            let fd = frame.rdi;
            let iov_ptr = frame.rsi as *const Iovec;
            let iovcnt = frame.rdx as usize;
            let mut written = 0;
            unsafe {
                for i in 0..iovcnt {
                    let iov = &*iov_ptr.add(i);
                    if fd == 1 || fd == 2 {
                        let slice = core::slice::from_raw_parts(iov.base, iov.len);
                        if let Ok(s) = core::str::from_utf8(slice) {
                            crate::serial_print!("{}", s);
                        }
                    }
                    written += iov.len;
                }
            }
            frame.rax = written as u64;
        }
        24 => { // sched_yield
            crate::task::yield_now();
            frame.rax = 0;
        }
        39 | 186 => { // getpid / gettid
            frame.rax = 100;
        }
        60 | 231 => { // exit / exit_group
            ELF_EXIT_REQUESTED.store(true, Ordering::SeqCst);
            unsafe {
                let saved_sp = KERNEL_SAVED_RSP;
                if saved_sp != 0 {
                    asm!(
                        "mov rsp, {sp}",
                        "pop r15", "pop r14", "pop r13", "pop r12", "pop rbx", "pop rbp",
                        "sti", "ret",
                        sp = in(reg) saved_sp,
                        options(noreturn)
                    );
                }
            }
            frame.rax = 0;
        }
        72 => { // fcntl (Stub: نجاح فوري للتحكم بالملفات)
            frame.rax = 0;
        }
        131 => { // sigaltstack (Stub: نجاح فوري لتخصيص مكدس الطوارئ)
            frame.rax = 0;
        }
        158 => { // arch_prctl
            if frame.rdi == 0x1002 { // ARCH_SET_FS
                unsafe { wrmsr(0xC0000100, frame.rsi); }
                frame.rax = 0;
            } else {
                frame.rax = -22i64 as u64; // EINVAL
            }
        }
        200 | 234 => { // tkill / tgkill
            log_warn!("MIRROR", "tkill/tgkill invoked. Terminating application safely.");
            unsafe {
                let saved_sp = KERNEL_SAVED_RSP;
                if saved_sp != 0 {
                    asm!(
                        "mov rsp, {sp}",
                        "pop r15", "pop r14", "pop r13", "pop r12", "pop rbx", "pop rbp",
                        "sti", "ret",
                        sp = in(reg) saved_sp,
                        options(noreturn)
                    );
                }
            }
            frame.rax = 0;
        }
        202 => { // futex
            frame.rax = 0;
        }
        218 => { // set_tid_address
            frame.rax = 100;
        }
        228 => { // clock_gettime
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
        318 => { // getrandom
            let buf = frame.rdi as *mut u8;
            let count = frame.rsi as usize;
            unsafe {
                for i in 0..count {
                    let t = pit::get_ticks().wrapping_mul(6364136223846793005).wrapping_add(i as u64);
                    *buf.add(i) = (t ^ (t >> 7)) as u8;
                }
            }
            frame.rax = count as u64;
        }

        // ===================================================================
        // ⚙️ EOS CUSTOM SYSCALLS (Shifted to 500+) ⚙️
        // ===================================================================
        500 => { // sys_sleep
            pit::sleep_ms(frame.rdi);
            frame.rax = 0;
        }
        501 => { // sys_clear_screen
            OVERLAY_ACTIVE.store(false, Ordering::SeqCst);
            frame.rax = 0;
        }
        502 => { // sys_blit_image_ptr
            let ptr = frame.rdi as *const u32;
            let src_w = frame.r10 as usize;
            let src_h = frame.r8 as usize;

            if !ptr.is_null() && src_w > 0 && src_h > 0 {
                let target_w = core::cmp::min(src_w, OVERLAY_MAX_W);
                let target_h = core::cmp::min(src_h, OVERLAY_MAX_H);

                unsafe {
                    let dest = addr_of_mut!(OVERLAY_PIXELS) as *mut u32;
                    for dy in 0..target_h {
                        let sy = (dy * src_h) / target_h;
                        for dx in 0..target_w {
                            let sx = (dx * src_w) / target_w;
                            let pixel = *ptr.add(sy * src_w + sx);
                            *dest.add(dy * target_w + dx) = pixel;
                        }
                    }
                }

                OVERLAY_WIDTH.store(target_w, Ordering::SeqCst);
                OVERLAY_HEIGHT.store(target_h, Ordering::SeqCst);
                OVERLAY_ACTIVE.store(true, Ordering::SeqCst);
            }
            frame.rax = 0;
        }
        _ => {
            log_error!("MIRROR", "UNHANDLED Syscall #{}: args=({:#x}, {:#x}, {:#x})", syscall_num, frame.rdi, frame.rsi, frame.rdx);
            frame.rax = -38i64 as u64; // ENOSYS
        }
    }
}
