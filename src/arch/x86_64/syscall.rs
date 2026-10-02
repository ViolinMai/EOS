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

pub static mut USERSPACE_EVENTS: [Option<InputEvent>; 128] = [None; 128];
pub static USER_EVENT_HEAD: AtomicUsize = AtomicUsize::new(0);
pub static USER_EVENT_TAIL: AtomicUsize = AtomicUsize::new(0);

pub fn push_user_event(ev: InputEvent) {
    let head = USER_EVENT_HEAD.load(Ordering::Relaxed);
    let next = (head + 1) % 128;
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

fn handle_open_common(path_str: &str) -> i64 {
    let clean = path_str.trim().strip_prefix("./").unwrap_or(path_str.trim());
    if let Ok(bytes) = crate::fs::vfs_read_bytes(clean) {
        unsafe {
            let proc = &mut *addr_of_mut!(CORE2_PROCESS);
            for (i, f) in proc.fd_table.iter_mut().enumerate().skip(3) {
                if f.is_none() {
                    *f = Some(FileDescriptor {
                        path: String::from(clean),
                        source: crate::task::FileSource::Memory(bytes),
                        offset: 0,
                    });
                    return i as i64;
                }
            }
        }
        -24i64 // -EMFILE
    } else {
        -2i64 // -ENOENT
    }
}

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
        2 => { // open (legacy)
            let mut buf = [0u8; 256];
            let mut len = 0;
            while len < 255 {
                let mut b = [0u8; 1];
                if copy_from_user(&mut b, frame.rdi + len, 1).is_err() { break; }
                if b[0] == 0 { break; }
                buf[len as usize] = b[0]; len += 1;
            }
            let path = core::str::from_utf8(&buf[..len as usize]).unwrap_or("");
            frame.rax = handle_open_common(path) as u64;
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
        5 => { // fstat (x86_64 struct stat: size = 144 bytes, st_size offset = 48)
            let fd = frame.rdi as usize;
            let statbuf = frame.rsi;
            if fd < 32 && validate_user_range(statbuf, 144) {
                unsafe {
                    let proc = &mut *addr_of_mut!(CORE2_PROCESS);
                    if let Some(file) = &proc.fd_table[fd] {
                        let size = match &file.source { crate::task::FileSource::Memory(b) => b.len() as u64, _ => 0 };
                        let mut st = [0u8; 144];
                        // st_mode = S_IFREG | 0644 (0100644)
                        let mode = 0o100644u32;
                        st[24..28].copy_from_slice(&mode.to_ne_bytes());
                        // st_size at byte 48
                        st[48..56].copy_from_slice(&size.to_ne_bytes());
                        let _ = copy_to_user(statbuf, &st);
                        frame.rax = 0;
                    } else { frame.rax = -9i64 as u64; }
                }
            } else { frame.rax = -9i64 as u64; }
        }
        7 | 271 => { // poll / ppoll
            let fds = frame.rdi;
            let n = frame.rsi as usize;
            if n <= 64 && validate_user_range(fds, n * 8) {
                for i in 0..n {
                    let _ = copy_to_user(fds + (i as u64) * 8 + 6, &[0u8, 0u8]);
                }
                frame.rax = 0;
            } else { frame.rax = -14i64 as u64; }
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
        16 => { // ioctl
            frame.rax = -25i64 as u64;
        }
        20 => { // writev
            let fd = frame.rdi;
            let iov_ptr = frame.rsi as *const [u64; 2];
            let iovcnt = frame.rdx as usize;
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
        35 | 226 | 230 => { // nanosleep
            let req = if frame.rax == 35 { frame.rdi } else { frame.rsi } as *const [u64; 2];
            if validate_user_range(req as u64, 16) {
                let mut times = [0u64; 2];
                if copy_from_user(unsafe { core::slice::from_raw_parts_mut(times.as_mut_ptr() as *mut u8, 16) }, req as u64, 16).is_ok() {
                    let ms = times[0].saturating_mul(1000).saturating_add(times[1] / 1_000_000);
                    crate::arch::x86_64::pit::sleep_ms(ms);
                    frame.rax = 0;
                } else { frame.rax = -14i64 as u64; }
            } else { frame.rax = -14i64 as u64; }
        }
        63 => { // uname
            let buf = frame.rdi;
            if validate_user_range(buf, 390) {
                let mut uts = [0u8; 390];
                uts[0..4].copy_from_slice(b"EOS\0");
                uts[65..71].copy_from_slice(b"0.1.0\0");
                uts[130..135].copy_from_slice(b"2026\0");
                uts[260..267].copy_from_slice(b"x86_64\0");
                let _ = copy_to_user(buf, &uts);
                frame.rax = 0;
            } else { frame.rax = -14i64 as u64; }
        }
        96 => { // gettimeofday
            let tv_ptr = frame.rdi;
            if validate_user_range(tv_ptr, 16) {
                let s = pit::get_uptime_seconds(); let ms = (pit::get_ticks() % 1000) * 1000;
                let tv = [s as u64, ms as u64];
                let _ = copy_to_user(tv_ptr, unsafe { core::slice::from_raw_parts(tv.as_ptr() as *const u8, 16) });
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
        186 | 218 => { frame.rax = 2; }
        228 => { // clock_gettime
            let tp_ptr = frame.rsi;
            if validate_user_range(tp_ptr, 16) {
                let s = pit::get_uptime_seconds(); let ns = (pit::get_ticks() % 1000) * 1_000_000;
                let tp = [s as u64, ns as u64];
                let _ = copy_to_user(tp_ptr, unsafe { core::slice::from_raw_parts(tp.as_ptr() as *const u8, 16) });
                frame.rax = 0;
            } else { frame.rax = -14i64 as u64; }
        }
        257 => { // openat: (dirfd: rdi, pathname: rsi, flags: rdx, mode: r10) - الأساسي لـ musl/Rust
            let mut buf = [0u8; 256];
            let mut len = 0;
            while len < 255 {
                let mut b = [0u8; 1];
                if copy_from_user(&mut b, frame.rsi + len, 1).is_err() { break; }
                if b[0] == 0 { break; }
                buf[len as usize] = b[0]; len += 1;
            }
            let path = core::str::from_utf8(&buf[..len as usize]).unwrap_or("");
            frame.rax = handle_open_common(path) as u64;
        }
        262 => { // newfstatat: (dirfd: rdi, pathname: rsi, statbuf: rdx, flags: r10)
            let mut buf = [0u8; 256];
            let mut len = 0;
            while len < 255 {
                let mut b = [0u8; 1];
                if copy_from_user(&mut b, frame.rsi + len, 1).is_err() { break; }
                if b[0] == 0 { break; }
                buf[len as usize] = b[0]; len += 1;
            }
            let path = core::str::from_utf8(&buf[..len as usize]).unwrap_or("");
            let clean = path.trim().strip_prefix("./").unwrap_or(path.trim());
            if let Ok(bytes) = crate::fs::vfs_read_bytes(clean) {
                let mut st = [0u8; 144];
                let mode = 0o100644u32;
                st[24..28].copy_from_slice(&mode.to_ne_bytes());
                let size = bytes.len() as u64;
                st[48..56].copy_from_slice(&size.to_ne_bytes());
                let _ = copy_to_user(frame.rdx, &st);
                frame.rax = 0;
            } else {
                frame.rax = -2i64 as u64;
            }
        }
        302 => { frame.rax = 0; }
        318 => { // getrandom
            let mut buf = alloc::vec![0u8; frame.rsi as usize];
            let mut r = pit::read_tsc();
            for b in buf.iter_mut() { *b = (r & 0xFF) as u8; r >>= 3; }
            if copy_to_user(frame.rdi, &buf).is_ok() { frame.rax = frame.rsi; } else { frame.rax = -14i64 as u64; }
        }
        502 => { // sys_present
            let target_w = core::cmp::min(frame.r10 as usize, 1920);
            let target_h = core::cmp::min(frame.r8 as usize, 1080);
            let total_bytes = target_w * target_h * 4;
            if target_w > 0 && target_h > 0 && validate_user_range(frame.rdi, total_bytes) {
                unsafe {
                    if let Some(writer) = &mut *core::ptr::addr_of_mut!(crate::writer::WRITER) {
                        let src = frame.rdi as *const u8;
                        let dst = writer.buffer;
                        let line_bytes = target_w.min(writer.width) * 4;
                        let copy_h = target_h.min(writer.height);
                        for y in 0..copy_h {
                            core::ptr::copy_nonoverlapping(
                                src.add(y * target_w * 4),
                                dst.add(y * writer.pitch),
                                line_bytes,
                            );
                        }
                    }
                }
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
                    USER_EVENT_TAIL.store((tail + 1) % 128, Ordering::Release);
                } else { frame.rax = 0; }
            } else { frame.rax = 0; }
        }
        10 | 11 | 13 | 14 | 24 | 28 | 131 | 202 | 273 | 334 => { frame.rax = 0; }
        39 => { frame.rax = 1; }
        60 | 231 => { ELF_EXIT_REQUESTED.store(true, Ordering::SeqCst); frame.rax = 0; }
        _ => {
            crate::log_warn!("SYSCALL", "Unimplemented syscall {} (rdi={:#x}, rsi={:#x})", frame.rax, frame.rdi, frame.rsi);
            frame.rax = -38i64 as u64;
        }
    }
}
