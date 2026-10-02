use core::arch::{asm, naked_asm};
use crate::arch::x86_64::pit;
use crate::task::FileDescriptor;
use crate::mm::paging::read_cr3;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use core::ptr::addr_of_mut;
use crate::input::InputEvent;
use alloc::string::String;

pub static mut KERNEL_SYSCALL_STACKS: [u64; 8] = [0; 8];
pub static ELF_EXIT_REQUESTED: AtomicBool = AtomicBool::new(false);

pub const OVERLAY_MAX_W: usize = 1920;
pub const OVERLAY_MAX_H: usize = 1080;
pub static OVERLAY_ACTIVE: AtomicBool = AtomicBool::new(false);
pub static OVERLAY_WIDTH: AtomicUsize = AtomicUsize::new(0);
pub static OVERLAY_HEIGHT: AtomicUsize = AtomicUsize::new(0);
pub static OVERLAY_LOCK: AtomicBool = AtomicBool::new(false);
pub static mut OVERLAY_PIXELS: [u32; OVERLAY_MAX_W * OVERLAY_MAX_H] = [0; OVERLAY_MAX_W * OVERLAY_MAX_H];

pub static mut USERSPACE_EVENTS: [Option<InputEvent>; 64] = [None; 64];
pub static USER_EVENT_HEAD: AtomicUsize = AtomicUsize::new(0);
pub static USER_EVENT_TAIL: AtomicUsize = AtomicUsize::new(0);

pub fn push_user_event(ev: InputEvent) {
    let head = USER_EVENT_HEAD.load(Ordering::Relaxed);
    let next = (head + 1) % 64;
    if next != USER_EVENT_TAIL.load(Ordering::Acquire) {
        unsafe { USERSPACE_EVENTS[head] = Some(ev); }
        USER_EVENT_HEAD.store(next, Ordering::Release);
    }
}

pub struct ProcessState {
    pub mmap_bump: u64,
    pub fd_table: [Option<FileDescriptor>; 32],
}
const EMPTY_FD: Option<FileDescriptor> = None;
pub static mut CORE2_PROCESS: ProcessState = ProcessState {
    mmap_bump: 0x0000_7000_0000_0000,
    fd_table: [EMPTY_FD; 32],
};

pub fn reset_core2_process() {
    unsafe {
        let proc = &mut *addr_of_mut!(CORE2_PROCESS);
        proc.mmap_bump = 0x0000_7000_0000_0000;
        for i in 0..32 { proc.fd_table[i] = None; }
        USER_EVENT_HEAD.store(0, Ordering::Relaxed);
        USER_EVENT_TAIL.store(0, Ordering::Relaxed);
    }
}

pub fn validate_user_range(ptr: u64, len: usize) -> bool {
    ptr.checked_add(len as u64).map_or(false, |end| end < 0x0000_8000_0000_0000)
}

pub fn copy_from_user(dst: &mut [u8], src: u64, len: usize) -> Result<(), ()> {
    if !validate_user_range(src, len) { return Err(()); }
    unsafe { core::ptr::copy_nonoverlapping(src as *const u8, dst.as_mut_ptr(), len); }
    Ok(())
}

pub fn copy_to_user(dst: u64, src: &[u8]) -> Result<(), ()> {
    if !validate_user_range(dst, src.len()) { return Err(()); }
    unsafe { core::ptr::copy_nonoverlapping(src.as_ptr(), dst as *mut u8, src.len()); }
    Ok(())
}

#[inline]
pub unsafe fn rdmsr(msr: u32) -> u64 {
    let (lo, hi): (u32, u32);
    unsafe { asm!("rdmsr", in("ecx") msr, out("eax") lo, out("edx") hi, options(nomem, nostack, preserves_flags)); }
    ((hi as u64) << 32) | (lo as u64)
}

#[inline]
pub unsafe fn wrmsr(msr: u32, value: u64) {
    let (lo, hi) = (value as u32, (value >> 32) as u32);
    unsafe { asm!("wrmsr", in("ecx") msr, in("eax") lo, in("edx") hi, options(nomem, nostack, preserves_flags)); }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SyscallFrame {
    pub rax: u64, pub rdi: u64, pub rsi: u64, pub rdx: u64,
    pub r10: u64, pub r8: u64, pub r9: u64, pub r11: u64, pub rcx: u64,
}

#[repr(C)]
pub struct CoreSyscallState {
    pub user_rsp: u64, pub kernel_rsp: u64, pub kernel_spawn_rsp: u64,
}

pub static mut CORE_SYSCALL_STATES: [CoreSyscallState; 8] = [
    CoreSyscallState { user_rsp: 0, kernel_rsp: 0, kernel_spawn_rsp: 0 },
    CoreSyscallState { user_rsp: 0, kernel_rsp: 0, kernel_spawn_rsp: 0 },
    CoreSyscallState { user_rsp: 0, kernel_rsp: 0, kernel_spawn_rsp: 0 },
    CoreSyscallState { user_rsp: 0, kernel_rsp: 0, kernel_spawn_rsp: 0 },
    CoreSyscallState { user_rsp: 0, kernel_rsp: 0, kernel_spawn_rsp: 0 },
    CoreSyscallState { user_rsp: 0, kernel_rsp: 0, kernel_spawn_rsp: 0 },
    CoreSyscallState { user_rsp: 0, kernel_rsp: 0, kernel_spawn_rsp: 0 },
    CoreSyscallState { user_rsp: 0, kernel_rsp: 0, kernel_spawn_rsp: 0 },
];

#[unsafe(naked)]
extern "C" fn syscall_entry() {
    naked_asm!(
        "swapgs", "mov gs:[0], rsp", "mov rsp, gs:[8]",
        "push rcx", "push r11", "push r9", "push r8", "push r10", "push rdx", "push rsi", "push rdi", "push rax",
        "push rbp", "push rbx", "push r12", "push r13", "push r14", "push r15",
        "sub rsp, 8", "lea rdi, [rsp + 56]",
        "call {handler}",
        "add rsp, 8",
        "cmp byte ptr [{exit_flag}], 1", "je 1f",
        "pop r15", "pop r14", "pop r13", "pop r12", "pop rbx", "pop rbp",
        "pop rax", "pop rdi", "pop rsi", "pop rdx", "pop r10", "pop r8", "pop r9", "pop r11", "pop rcx",
        "mov rsp, gs:[0]", "swapgs", "sysretq",
        "1:", "mov rsp, gs:[16]", "pop r15", "pop r14", "pop r13", "pop r12", "pop rbx", "pop rbp", "swapgs", "ret",
        handler = sym syscall_handler, exit_flag = sym ELF_EXIT_REQUESTED,
    );
}

pub fn init_core_syscall(core_id: usize) {
    unsafe {
        let efer = rdmsr(0xC0000080); wrmsr(0xC0000080, efer | 1);
        let star = (0x0008u64 << 32) | (0x0010u64 << 48); wrmsr(0xC0000081, star);
        wrmsr(0xC0000082, syscall_entry as *const () as u64);
        wrmsr(0xC0000084, 0x200u64);
        let state_ptr = addr_of_mut!(CORE_SYSCALL_STATES[core_id]);
        if core_id < 8 && KERNEL_SYSCALL_STACKS[core_id] != 0 {
            (*state_ptr).kernel_rsp = KERNEL_SYSCALL_STACKS[core_id];
        }
        wrmsr(0xC0000102, state_ptr as u64); 
    }
}

pub fn init() { init_core_syscall(0); }

#[unsafe(no_mangle)]
pub extern "C" fn syscall_handler(frame_ptr: *mut SyscallFrame) {
    let frame = unsafe { &mut *frame_ptr };
    match frame.rax {
        0 => { // read
            let fd = frame.rdi as usize;
            if fd < 32 {
                unsafe {
                    let proc = &mut *addr_of_mut!(CORE2_PROCESS);
                    if let Some(file) = &mut proc.fd_table[fd] {
                        match &file.source {
                            crate::task::FileSource::Memory(bytes) => {
                                let remain = bytes.len().saturating_sub(file.offset);
                                let to_copy = remain.min(frame.rdx as usize);
                                if to_copy > 0 {
                                    if copy_to_user(frame.rsi, &bytes[file.offset..file.offset+to_copy]).is_ok() {
                                        file.offset += to_copy; frame.rax = to_copy as u64;
                                    } else { frame.rax = -14i64 as u64; }
                                } else { frame.rax = 0; }
                            },
                            _ => { frame.rax = -38i64 as u64; }
                        }
                    } else { frame.rax = -9i64 as u64; }
                }
            } else { frame.rax = -9i64 as u64; }
        }
        1 => { // write
            if frame.rdi == 1 || frame.rdi == 2 {
                let mut buf = alloc::vec![0u8; frame.rdx as usize];
                if copy_from_user(&mut buf, frame.rsi, frame.rdx as usize).is_ok() {
                    if let Ok(s) = core::str::from_utf8(&buf) { crate::serial_print!("{}", s); }
                    frame.rax = frame.rdx;
                } else { frame.rax = -14i64 as u64; }
            } else { frame.rax = -9i64 as u64; }
        }
        2 => { // open
            let mut buf = [0u8; 256];
            let mut len = 0;
            while len < 255 {
                let mut b = [0u8; 1];
                if copy_from_user(&mut b, frame.rdi + len, 1).is_err() { break; }
                if b[0] == 0 { break; }
                buf[len as usize] = b[0]; len += 1;
            }
            let path = core::str::from_utf8(&buf[..len as usize]).unwrap_or("");
            if let Ok(bytes) = crate::fs::vfs_read_bytes(path) {
                let mut fd_out = -24i64;
                unsafe {
                    let proc = &mut *addr_of_mut!(CORE2_PROCESS);
                    for (i, f) in proc.fd_table.iter_mut().enumerate().skip(3) {
                        if f.is_none() {
                            *f = Some(FileDescriptor {
                                path: String::from(path),
                                source: crate::task::FileSource::Memory(bytes),
                                offset: 0,
                            });
                            fd_out = i as i64; break;
                        }
                    }
                }
                frame.rax = fd_out as u64;
            } else { frame.rax = -2i64 as u64; }
        }
        3 => { // close
            let fd = frame.rdi as usize;
            if fd < 32 {
                unsafe {
                    let proc = &mut *addr_of_mut!(CORE2_PROCESS);
                    proc.fd_table[fd] = None;
                    frame.rax = 0;
                }
            } else { frame.rax = -9i64 as u64; }
        }
        8 => { // lseek
            let fd = frame.rdi as usize;
            if fd < 32 {
                unsafe {
                    let proc = &mut *addr_of_mut!(CORE2_PROCESS);
                    if let Some(file) = &mut proc.fd_table[fd] {
                        let offset = frame.rsi as i64;
                        let whence = frame.rdx;
                        let size = match &file.source { crate::task::FileSource::Memory(b) => b.len(), _ => 0 };
                        let new_off = match whence { 0 => offset, 1 => file.offset as i64 + offset, 2 => size as i64 + offset, _ => -1 };
                        if new_off >= 0 { file.offset = new_off as usize; frame.rax = new_off as u64; }
                        else { frame.rax = -22i64 as u64; }
                    } else { frame.rax = -9i64 as u64; }
                }
            } else { frame.rax = -9i64 as u64; }
        }
        9 => { // mmap
            let len = frame.rsi as usize; let flags = frame.r10;
            if (flags & 0x20) != 0 {
                let pages = (len + 0xFFF) / 4096;
                let vaddr = unsafe {
                    let proc = &mut *addr_of_mut!(CORE2_PROCESS);
                    let addr = proc.mmap_bump;
                    proc.mmap_bump += (pages * 4096) as u64;
                    addr
                };
                let cur_pml4 = read_cr3() & 0x000F_FFFF_FFFF_F000;
                if crate::mm::paging::map_user_pages(cur_pml4, vaddr, pages).is_err() { frame.rax = -12i64 as u64; }
                else { frame.rax = vaddr; }
            } else { frame.rax = -38i64 as u64; }
        }
        10 | 11 | 13 | 14 | 16 | 302 => { frame.rax = 0; }
        12 => { // brk
            let addr = frame.rdi;
            unsafe {
                let proc = &mut *addr_of_mut!(CORE2_PROCESS);
                if addr == 0 { frame.rax = proc.mmap_bump; } else {
                    let current = proc.mmap_bump;
                    if addr > current {
                        let pages = ((addr - current) as usize + 0xFFF) / 4096;
                        let cur_pml4 = read_cr3() & 0x000F_FFFF_FFFF_F000;
                        if crate::mm::paging::map_user_pages(cur_pml4, current, pages).is_err() {
                            frame.rax = current; return;
                        }
                        proc.mmap_bump = (addr + 0xFFF) & !0xFFF;
                    }
                    frame.rax = addr;
                }
            }
        }
        20 => { // writev
            let fd = frame.rdi; let iov_ptr = frame.rsi as *const [u64; 2]; let iovcnt = frame.rdx as usize;
            if fd == 1 || fd == 2 {
                let mut written = 0;
                for i in 0..iovcnt {
                    let mut iov = [0u64; 2];
                    if copy_from_user(unsafe { core::slice::from_raw_parts_mut(iov.as_mut_ptr() as *mut u8, 16) }, iov_ptr as u64 + (i * 16) as u64, 16).is_ok() {
                        let mut buf = alloc::vec![0u8; iov[1] as usize];
                        if copy_from_user(&mut buf, iov[0], iov[1] as usize).is_ok() {
                            if let Ok(s) = core::str::from_utf8(&buf) { crate::serial_print!("{}", s); written += iov[1]; }
                        }
                    }
                }
                frame.rax = written;
            } else { frame.rax = -9i64 as u64; }
        }
        24 | 202 => { crate::task::yield_now(); frame.rax = 0; }
        39 => { frame.rax = 1; }
        60 | 231 => { ELF_EXIT_REQUESTED.store(true, Ordering::SeqCst); frame.rax = 0; }
        96 => { // gettimeofday
            let tv_ptr = frame.rdi;
            if validate_user_range(tv_ptr, 16) {
                let s = pit::get_uptime_seconds(); let ms = (pit::get_ticks() % 1000) * 1000;
                let tv = [s as u64, ms as u64];
                let _ = copy_to_user(tv_ptr, unsafe { core::slice::from_raw_parts(tv.as_ptr() as *const u8, 16) });
                frame.rax = 0;
            } else { frame.rax = -14i64 as u64; }
        }
        228 => { // clock_gettime
            let tp_ptr = frame.rsi;
            if validate_user_range(tp_ptr, 16) {
                let s = pit::get_uptime_seconds(); let ns = (pit::get_ticks() % 1000) * 1_000_000;
                let tp = [s as u64, ns as u64];
                let _ = copy_to_user(tp_ptr, unsafe { core::slice::from_raw_parts(tp.as_ptr() as *const u8, 16) });
                frame.rax = 0;
            } else { frame.rax = -14i64 as u64; }
        }
        158 => { // arch_prctl
            match frame.rdi {
                0x1002 => { unsafe { wrmsr(0xC0000100, frame.rsi); } frame.rax = 0; }
                0x1001 => { unsafe { wrmsr(0xC0000102, frame.rsi); } frame.rax = 0; }
                _ => frame.rax = -38i64 as u64,
            }
        }
        318 => { // getrandom
            let mut buf = alloc::vec![0u8; frame.rsi as usize];
            let mut r = pit::read_tsc();
            for b in buf.iter_mut() { *b = (r & 0xFF) as u8; r >>= 3; }
            if copy_to_user(frame.rdi, &buf).is_ok() { frame.rax = frame.rsi; } else { frame.rax = -14i64 as u64; }
        }
        502 => { // sys_present
            let target_w = core::cmp::min(frame.r10 as usize, OVERLAY_MAX_W);
            let target_h = core::cmp::min(frame.r8 as usize, OVERLAY_MAX_H);
            if target_w > 0 && target_h > 0 && validate_user_range(frame.rdi, target_w * target_h * 4) {
                while OVERLAY_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() { core::hint::spin_loop(); }
                unsafe { core::ptr::copy_nonoverlapping(frame.rdi as *const u32, addr_of_mut!(OVERLAY_PIXELS) as *mut u32, target_w * target_h); }
                OVERLAY_WIDTH.store(target_w, Ordering::Relaxed); OVERLAY_HEIGHT.store(target_h, Ordering::Relaxed);
                OVERLAY_ACTIVE.store(true, Ordering::Release); OVERLAY_LOCK.store(false, Ordering::Release);
            }
            frame.rax = 0;
        }
        503 => { // sys_poll_event
            let tail = USER_EVENT_TAIL.load(Ordering::Acquire); let head = USER_EVENT_HEAD.load(Ordering::Acquire);
            if head != tail {
                if let Some(ev) = unsafe { USERSPACE_EVENTS[tail] } {
                    match ev {
                        InputEvent::MouseMove { x, y } => { frame.rax = 1; frame.rdi = ((x as u32) as u64) | (((y as u32) as u64) << 32); },
                        InputEvent::MouseButton { button, pressed } => { frame.rax = 2; frame.rdi = (button as u64) | (if pressed { 1 << 8 } else { 0 }); },
                        InputEvent::KeyDown { keycode, mods } => { frame.rax = 3; frame.rdi = (keycode as u64) | ((mods as u64) << 8); },
                        InputEvent::Char(c) => { frame.rax = 4; frame.rdi = c as u64; },
                        _ => { frame.rax = 0; }
                    }
                    USER_EVENT_TAIL.store((tail + 1) % 64, Ordering::Release);
                } else { frame.rax = 0; }
            } else { frame.rax = 0; }
        }
        _ => { frame.rax = -38i64 as u64; }
    }
}
