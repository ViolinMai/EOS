use core::arch::{asm, naked_asm};
use crate::arch::x86_64::pit;
use crate::task::FileDescriptor;
use crate::mm::paging::read_cr3;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use core::ptr::addr_of_mut;
use crate::input::InputEvent;

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
    let tail = USER_EVENT_TAIL.load(Ordering::Acquire);

    // Merge consecutive MouseMove events in the queue to save space and reduce latency
    if let InputEvent::MouseMove { x: new_dx, y: new_dy } = ev {
        if head != tail {
            let prev_idx = if head == 0 { 127 } else { head - 1 };
            if let Some(InputEvent::MouseMove { x, y }) = unsafe { USERSPACE_EVENTS[prev_idx].as_mut() } {
                *x += new_dx;
                *y += new_dy;
                return;
            }
        }
    }

    let next = (head + 1) % 128;
    if next != tail {
        unsafe { USERSPACE_EVENTS[head] = Some(ev); }
        USER_EVENT_HEAD.store(next, Ordering::Release);
    }
}

pub struct ProcessState {
    pub mmap_bump: u64,
    pub fd_table: [Option<FileDescriptor>; 64],
}

pub fn reset_core2_process() {
    unsafe {
        let core_id = crate::profiler::get_core_id();
        CURRENT_PID[core_id].store(core_id, core::sync::atomic::Ordering::Release);
        PROCESS_LOCK.lock();
        PROCESS_TABLE[core_id] = Some(ProcessState {
            mmap_bump: 0x0000_7000_0000_0000,
            fd_table: [const { None }; 64],
        });
        PROCESS_LOCK.unlock();
        USER_EVENT_HEAD.store(0, core::sync::atomic::Ordering::Relaxed);
        USER_EVENT_TAIL.store(0, core::sync::atomic::Ordering::Relaxed);
    }
}

pub fn validate_user_range(ptr: u64, len: usize) -> bool {
    if ptr == 0 || len == 0 { return false; }
    let end = match ptr.checked_add(len as u64) {
        Some(e) => e,
        None => return false,
    };
    if end >= 0x0000_8000_0000_0000 { return false; }

    let start_page = ptr & !0xFFF;
    let end_page = (end + 0xFFF) & !0xFFF;
    let cr3 = crate::mm::paging::read_cr3() & 0x000F_FFFF_FFFF_F000;

    let mut curr = start_page;
    while curr < end_page {
        if !crate::mm::paging::is_user_mapped(cr3, curr) { return false; }
        curr += 4096;
    }
    true
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
pub unsafe fn wrmsr(msr: u32, value: u64) {
    let (lo, hi) = (value as u32, (value >> 32) as u32);
    unsafe { asm!("wrmsr", in("ecx") msr, in("eax") lo, in("edx") hi, options(nomem, nostack, preserves_flags)); }
}

#[inline]
pub unsafe fn rdmsr(msr: u32) -> u64 {
    let (lo, hi): (u32, u32);
    unsafe { asm!("rdmsr", in("ecx") msr, out("eax") lo, out("edx") hi, options(nomem, nostack, preserves_flags)); }
    ((hi as u64) << 32) | (lo as u64)
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

fn fill_stat_buffer(statbuf: u64, size: u64, mode: u32) -> Result<(), ()> {
    let mut st = [0u8; 144];
    st[24..28].copy_from_slice(&mode.to_ne_bytes());
    st[48..56].copy_from_slice(&size.to_ne_bytes());
    copy_to_user(statbuf, &st)
}

#[unsafe(no_mangle)]
pub fn syscall_handler(frame_ptr: *mut SyscallFrame) {
    let frame = unsafe { &mut *frame_ptr };
    let cur_core = crate::profiler::get_core_id();

    match frame.rax {
        0 => { // read
            let fd = frame.rdi as usize;
            if fd < 64 {
                let proc = get_current_process();
                if let Some(file) = &mut proc.fd_table[fd] {
                    match &mut file.source {
                        crate::task::FileSource::Memory(bytes) => {
                            let remain = bytes.len().saturating_sub(file.offset);
                            let to_copy = remain.min(frame.rdx as usize);
                            if to_copy > 0 {
                                if copy_to_user(frame.rsi, &bytes[file.offset..file.offset+to_copy]).is_ok() {
                                    file.offset += to_copy; frame.rax = to_copy as u64;
                                } else { frame.rax = -14i64 as u64; }
                            } else { frame.rax = 0; }
                        },
                        crate::task::FileSource::Pipe { id, is_read } => {
                            if !*is_read { frame.rax = -9i64 as u64; return; }
                            crate::task::PIPE_LOCK.lock();
                            if let Some(pipe) = unsafe { &mut crate::task::PIPE_REGISTRY[*id] } {
                                let to_read = core::cmp::min(pipe.data.len(), frame.rdx as usize);
                                if to_read > 0 {
                                    let mut buf = alloc::vec![0u8; to_read];
                                    for i in 0..to_read {
                                        buf[i] = pipe.data.pop_front().unwrap();
                                    }
                                    crate::task::PIPE_LOCK.unlock();
                                    if copy_to_user(frame.rsi, &buf).is_ok() {
                                        frame.rax = to_read as u64;
                                    } else {
                                        frame.rax = -14i64 as u64;
                                    }
                                } else {
                                    crate::task::PIPE_LOCK.unlock();
                                    if pipe.closed { frame.rax = 0; }
                                    else { crate::task::yield_now(); frame.rax = -11i64 as u64; }
                                }
                            } else {
                                crate::task::PIPE_LOCK.unlock();
                                frame.rax = -9i64 as u64;
                            }
                        },
                        _ => { frame.rax = -38i64 as u64; }
                    }
                } else { frame.rax = -9i64 as u64; }
            } else { frame.rax = -9i64 as u64; }
        }
        1 => { // write
            let fd = frame.rdi as usize;
            if fd == 1 || fd == 2 {
                let mut buf = alloc::vec![0u8; frame.rdx as usize];
                if copy_from_user(&mut buf, frame.rsi, frame.rdx as usize).is_ok() {
                    if let Ok(s) = core::str::from_utf8(&buf) { crate::serial_print!("{}", s); }
                    frame.rax = frame.rdx;
                } else { frame.rax = -14i64 as u64; }
            } else if fd < 64 {
                let proc = get_current_process();
                if let Some(file) = &mut proc.fd_table[fd] {
                    if !file.is_writable {
                        frame.rax = -9i64 as u64;
                        return;
                    }
                    match &mut file.source {
                        crate::task::FileSource::Memory(bytes) => {
                            let mut buf = alloc::vec![0u8; frame.rdx as usize];
                            if copy_from_user(&mut buf, frame.rsi, frame.rdx as usize).is_ok() {
                                if file.offset + buf.len() > bytes.len() { bytes.resize(file.offset + buf.len(), 0); }
                                bytes[file.offset..file.offset+buf.len()].copy_from_slice(&buf);
                                file.offset += buf.len();
                                file.is_dirty = true;
                                frame.rax = buf.len() as u64;
                            } else { frame.rax = -14i64 as u64; }
                        },
                        crate::task::FileSource::Pipe { id, is_read } => {
                            if *is_read { frame.rax = -9i64 as u64; return; }
                            let mut buf = alloc::vec![0u8; frame.rdx as usize];
                            if copy_from_user(&mut buf, frame.rsi, frame.rdx as usize).is_ok() {
                                crate::task::PIPE_LOCK.lock();
                                if let Some(pipe) = unsafe { &mut crate::task::PIPE_REGISTRY[*id] } {
                                    if pipe.closed {
                                        crate::task::PIPE_LOCK.unlock();
                                        frame.rax = -32i64 as u64;
                                    } else {
                                        pipe.data.extend(buf.into_iter());
                                        crate::task::PIPE_LOCK.unlock();
                                        frame.rax = frame.rdx;
                                    }
                                } else {
                                    crate::task::PIPE_LOCK.unlock();
                                    frame.rax = -9i64 as u64;
                                }
                            } else {
                                frame.rax = -14i64 as u64;
                            }
                        },
                        _ => { frame.rax = -9i64 as u64; }
                    }
                } else { frame.rax = -9i64 as u64; }
            } else { frame.rax = -9i64 as u64; }
        }
        2 | 257 => { // open, openat
            let (path_ptr, flags) = if frame.rax == 2 { (frame.rdi, frame.rsi) } else { (frame.rsi, frame.rdx) };
            let mut buf = [0u8; 512];
            let mut len = 0;
            while len < 511 {
                let mut b = [0u8; 1];
                if copy_from_user(&mut b, path_ptr + len, 1).is_err() || b[0] == 0 { break; }
                buf[len as usize] = b[0]; len += 1;
            }
            let path = core::str::from_utf8(&buf[..len as usize]).unwrap_or("").trim();
            let clean_path = path.trim_matches('/');

            let o_accmode = flags & 3;
            let o_wronly = o_accmode == 1;
            let o_rdwr = o_accmode == 2;
            let is_writable = o_wronly || o_rdwr;
            let o_creat = (flags & 0o100) != 0;
            let o_trunc = (flags & 0o1000) != 0;
            let o_append = (flags & 0o2000) != 0;
            let is_dir_req = (flags & 65536) != 0;

            let mut opened = false;
            let mut src = crate::task::FileSource::Memory(alloc::vec![]);
            let mut init_offset = 0usize;

            if clean_path == "dev/urandom" || clean_path == "dev/random" {
                let mut rand_bytes = alloc::vec![0u8; 1024];
                let mut r = pit::read_tsc();
                for b in rand_bytes.iter_mut() {
                    *b = (r & 0xFF) as u8;
                    r = r.wrapping_mul(6364136223846793005).wrapping_add(1);
                }
                src = crate::task::FileSource::Memory(rand_bytes);
                opened = true;
            } else if is_dir_req {
                if let Ok(items) = crate::fs::vfs_list_dir(path) {
                    src = crate::task::FileSource::Directory(items, 0);
                    opened = true;
                }
            } else if let Ok(bytes) = crate::fs::vfs_read_bytes(path) {
                init_offset = if o_append { bytes.len() } else { 0 };
                src = crate::task::FileSource::Memory(if o_trunc { alloc::vec![] } else { bytes });
                opened = true;
            } else if let Ok(items) = crate::fs::vfs_list_dir(path) {
                src = crate::task::FileSource::Directory(items, 0);
                opened = true;
            } else if o_creat {
                src = crate::task::FileSource::Memory(alloc::vec![]);
                opened = true;
            }

            if opened {
                let proc = get_current_process();
                let mut fd_allocated = false;
                for (i, f) in proc.fd_table.iter_mut().enumerate().skip(3) {
                    if f.is_none() {
                        *f = Some(crate::task::FileDescriptor {
                            path: alloc::string::String::from(path),
                            source: src,
                            offset: init_offset,
                            is_writable,
                            is_dirty: o_trunc,
                        });
                        frame.rax = i as u64;
                        fd_allocated = true;
                        break;
                    }
                }
                if !fd_allocated { frame.rax = -24i64 as u64; }
            } else {
                frame.rax = -2i64 as u64;
            }
        }
        3 | 6 => { // close
            let fd = frame.rdi as usize;
            if fd < 64 {
                let proc = get_current_process();
                if let Some(file) = proc.fd_table[fd].take() {
                    if file.is_writable && file.is_dirty {
                        if let crate::task::FileSource::Memory(ref bytes) = file.source {
                            let _ = crate::fs::vfs_save_text_file(&file.path, bytes);
                        }
                    }
                    if let crate::task::FileSource::Pipe { id, .. } = file.source {
                        crate::task::PIPE_LOCK.lock();
                        if let Some(pipe) = unsafe { &mut crate::task::PIPE_REGISTRY[id] } {
                            pipe.closed = true;
                        }
                        crate::task::PIPE_LOCK.unlock();
                    }
                }
                frame.rax = 0;
            } else { frame.rax = -9i64 as u64; }
        }
        4 => { // stat
            let path_ptr = frame.rdi;
            let statbuf = frame.rsi;
            let mut buf = [0u8; 512];
            let mut len = 0;
            while len < 511 {
                let mut b = [0u8; 1];
                if copy_from_user(&mut b, path_ptr + len, 1).is_err() || b[0] == 0 { break; }
                buf[len as usize] = b[0]; len += 1;
            }
            let path = core::str::from_utf8(&buf[..len as usize]).unwrap_or("").trim();
            if let Ok((sz, kind)) = crate::fs::vfs_stat(path) {
                if fill_stat_buffer(statbuf, sz, kind).is_ok() { frame.rax = 0; } else { frame.rax = -14i64 as u64; }
            } else { frame.rax = -2i64 as u64; }
        }
        5 | 262 => { // fstat, newfstatat
            let (fd, path_ptr, statbuf) = if frame.rax == 5 { (frame.rdi as usize, 0, frame.rsi) } else { (frame.rdi as usize, frame.rsi, frame.rdx) };
            let mut is_ok = false;
            let mut mode = 0o100644u32;
            let mut size = 0u64;

            if frame.rax == 262 && path_ptr != 0 {
                let mut buf = [0u8; 512];
                let mut len = 0;
                while len < 511 {
                    let mut b = [0u8; 1];
                    if copy_from_user(&mut b, path_ptr + len, 1).is_err() || b[0] == 0 { break; }
                    buf[len as usize] = b[0]; len += 1;
                }
                let path = core::str::from_utf8(&buf[..len as usize]).unwrap_or("").trim();
                if let Ok((sz, kind)) = crate::fs::vfs_stat(path) {
                    size = sz; mode = kind; is_ok = true;
                }
            } else if fd < 64 {
                let proc = get_current_process();
                if let Some(file) = &proc.fd_table[fd] {
                    match &file.source {
                        crate::task::FileSource::Memory(b) => { size = b.len() as u64; mode = 0o100644; is_ok = true; },
                        crate::task::FileSource::Directory(_,_) => { size = 0; mode = 0o040755; is_ok = true; },
                        crate::task::FileSource::AtaDisk { size: s, .. } => { size = *s as u64; mode = 0o100644; is_ok = true; },
                        crate::task::FileSource::Pipe { .. } => { size = 0; mode = 0o010600; is_ok = true; },
                    }
                }
            }
            if is_ok {
                if fill_stat_buffer(statbuf, size, mode).is_ok() { frame.rax = 0; } else { frame.rax = -14i64 as u64; }
            } else { frame.rax = -2i64 as u64; }
        }
        7 | 271 => { // poll, ppoll
            let fds = frame.rdi; let n = frame.rsi as usize;
            if n <= 64 && validate_user_range(fds, n * 8) {
                for i in 0..n { let _ = copy_to_user(fds + (i as u64) * 8 + 6, &[0u8, 0u8]); }
                frame.rax = 0;
            } else { frame.rax = -14i64 as u64; }
        }
        8 => { // lseek
            let fd = frame.rdi as usize;
            if fd < 64 {
                let proc = get_current_process();
                if let Some(file) = &mut proc.fd_table[fd] {
                    let offset = frame.rsi as i64; let whence = frame.rdx;
                    let size = match &file.source { crate::task::FileSource::Memory(b) => b.len(), _ => 0 };
                    let new_off = match whence { 0 => offset, 1 => file.offset as i64 + offset, 2 => size as i64 + offset, _ => -1 };
                    if new_off >= 0 { file.offset = new_off as usize; frame.rax = new_off as u64; } else { frame.rax = -22i64 as u64; }
                } else { frame.rax = -9i64 as u64; }
            } else { frame.rax = -9i64 as u64; }
        }
        9 => { // mmap
            let len = frame.rsi as usize; let flags = frame.r10;
            if (flags & 0x20) != 0 || len > 0 {
                let pages = (len + 0xFFF) / 4096;
                let proc = get_current_process();
                let vaddr = proc.mmap_bump;
                proc.mmap_bump += (pages * 4096) as u64;
                let cur_pml4 = read_cr3() & 0x000F_FFFF_FFFF_F000;
                if crate::mm::paging::map_user_pages(cur_pml4, vaddr, pages).is_err() { frame.rax = -12i64 as u64; } else { frame.rax = vaddr; }
            } else { frame.rax = -38i64 as u64; }
        }
        11 => { frame.rax = 0; }
        12 => { // brk
            let addr = frame.rdi;
            let proc = get_current_process();
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
        16 => { // ioctl
            let req = frame.rsi;
            if req == 0x5413 { // TIOCGWINSZ
                let ws_ptr = frame.rdx;
                if validate_user_range(ws_ptr, 8) {
                    let winsize: [u16; 4] = [25, 80, 0, 0];
                    let _ = copy_to_user(ws_ptr, unsafe { core::slice::from_raw_parts(winsize.as_ptr() as *const u8, 8) });
                    frame.rax = 0;
                } else {
                    frame.rax = -14i64 as u64;
                }
            } else {
                frame.rax = 0;
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
                            if let Ok(s) = core::str::from_utf8(&buf) { crate::serial_print!("{}", s); }
                            written += iov[1];
                        }
                    }
                }
                frame.rax = written;
            } else { frame.rax = -9i64 as u64; }
        }
        22 => { // pipe
            let pipefd_ptr = frame.rdi;
            if validate_user_range(pipefd_ptr, 8) {
                if let Some((r_id, w_id)) = crate::task::create_pipe() {
                    let proc = get_current_process();
                    let mut r_fd = None;
                    let mut w_fd = None;
                    for (i, f) in proc.fd_table.iter_mut().enumerate().skip(3) {
                        if f.is_none() && r_fd.is_none() { r_fd = Some(i); continue; }
                        if f.is_none() && w_fd.is_none() { w_fd = Some(i); break; }
                    }
                    if let (Some(r), Some(w)) = (r_fd, w_fd) {
                        proc.fd_table[r] = Some(crate::task::FileDescriptor {
                            path: alloc::string::String::from("pipe:read"),
                            source: crate::task::FileSource::Pipe { id: r_id, is_read: true },
                            offset: 0, is_writable: false, is_dirty: false,
                        });
                        proc.fd_table[w] = Some(crate::task::FileDescriptor {
                            path: alloc::string::String::from("pipe:write"),
                            source: crate::task::FileSource::Pipe { id: w_id, is_read: false },
                            offset: 0, is_writable: true, is_dirty: false,
                        });
                        let fds = [(r as u32), (w as u32)];
                        let _ = copy_to_user(pipefd_ptr, unsafe { core::slice::from_raw_parts(fds.as_ptr() as *const u8, 8) });
                        frame.rax = 0;
                    } else {
                        frame.rax = -24i64 as u64;
                    }
                } else {
                    frame.rax = -23i64 as u64;
                }
            } else {
                frame.rax = -14i64 as u64;
            }
        }
        24 => { // sched_yield
            crate::task::yield_now();
            frame.rax = 0;
        }
        60 => { // exit
            if cur_core == 2 {
                ELF_EXIT_REQUESTED.store(true, Ordering::SeqCst);
            } else {
                let ctid_addr = THREAD_CLEAR_TID[cur_core].swap(0, Ordering::SeqCst);
                if ctid_addr != 0 {
                    let zero = 0u32;
                    let _ = copy_to_user(ctid_addr, &zero.to_ne_bytes());
                }
                THREAD_ACTIVE[cur_core].store(false, Ordering::SeqCst);
                crate::task::force_exit_user_process();
            }
            frame.rax = 0;
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
        79 => { // getcwd
            let buf = frame.rdi; let size = frame.rsi as usize; let cwd = b"/\0";
            if size >= cwd.len() && copy_to_user(buf, cwd).is_ok() { frame.rax = buf; } else { frame.rax = -34i64 as u64; }
        }
        89 => { // readlink
            let buf = frame.rsi;
            let bufsiz = frame.rdx as usize;
            if bufsiz > 0 && validate_user_range(buf, 1) {
                let dummy = b"/bin/python\0";
                let to_copy = bufsiz.min(dummy.len());
                let _ = copy_to_user(buf, &dummy[..to_copy]);
                frame.rax = to_copy as u64;
            } else {
                frame.rax = -2i64 as u64;
            }
        }
        96 => { // gettimeofday
            let tv_ptr = frame.rdi;
            if validate_user_range(tv_ptr, 16) {
                let rtc = crate::drivers::rtc::get_riyadh_time();
                let s = (rtc.hour as u64 * 3600) + (rtc.minute as u64 * 60) + (rtc.second as u64);
                let ms = (pit::get_ticks() % 1000) * 1000;
                let tv = [s, ms as u64];
                let _ = copy_to_user(tv_ptr, unsafe { core::slice::from_raw_parts(tv.as_ptr() as *const u8, 16) });
                frame.rax = 0;
            } else { frame.rax = -14i64 as u64; }
        }
        102 | 104 => { frame.rax = 1000; } // getuid, getgid (march)
        107 | 108 => { frame.rax = 1000; } // geteuid, getegid
        158 => { // arch_prctl
            match frame.rdi {
                0x1002 => { unsafe { wrmsr(0xC0000100, frame.rsi); } frame.rax = 0; }
                0x1001 => { unsafe { wrmsr(0xC0000102, frame.rsi); } frame.rax = 0; }
                _ => frame.rax = -38i64 as u64,
            }
        }
        186 => { frame.rax = cur_core as u64; } // gettid
        218 => { // set_tid_address
            let tidptr = frame.rdi;
            THREAD_CLEAR_TID[cur_core].store(tidptr, Ordering::SeqCst);
            frame.rax = cur_core as u64;
        }
        228 => { // clock_gettime
            let tp_ptr = frame.rsi;
            if validate_user_range(tp_ptr, 16) {
                let s = pit::get_uptime_seconds(); let ns = (pit::get_ticks() % 1000) * 1_000_000; let tp = [s as u64, ns as u64];
                let _ = copy_to_user(tp_ptr, unsafe { core::slice::from_raw_parts(tp.as_ptr() as *const u8, 16) }); frame.rax = 0;
            } else { frame.rax = -14i64 as u64; }
        }
        231 => { // exit_group
            ELF_EXIT_REQUESTED.store(true, Ordering::SeqCst);
            frame.rax = 0;
        }
        318 => { // getrandom
            let mut buf = alloc::vec![0u8; frame.rsi as usize];
            let mut r = pit::read_tsc();
            for b in buf.iter_mut() {
                *b = (r & 0xFF) as u8;
                r = r.wrapping_mul(6364136223846793005).wrapping_add(1);
            }
            if copy_to_user(frame.rdi, &buf).is_ok() {
                frame.rax = frame.rsi;
            } else {
                frame.rax = -14i64 as u64;
            }
        }
        500 => { // sys_spawn_elf
            let path_ptr = frame.rdi;
            let args_ptr = frame.rsi;
            let mut p_buf = [0u8; 128];
            let mut a_buf = [0u8; 128];
            let _ = copy_from_user(&mut p_buf, path_ptr, 127);
            let _ = copy_from_user(&mut a_buf, args_ptr, 127);
            let path_len = p_buf.iter().position(|&b| b == 0).unwrap_or(127);
            let args_len = a_buf.iter().position(|&b| b == 0).unwrap_or(127);
            let path_str = core::str::from_utf8(&p_buf[..path_len]).unwrap_or("");
            let args_str = core::str::from_utf8(&a_buf[..args_len]).unwrap_or("");

            match crate::task::request_elf_execution(path_str, args_str) {
                Ok(core_id) => { frame.rax = core_id as u64; }
                Err(_) => { frame.rax = -2i64 as u64; }
            }
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
                        for y in 0..target_h.min(writer.height) {
                            core::ptr::copy_nonoverlapping(src.add(y * target_w * 4), dst.add(y * writer.pitch), line_bytes);
                        }
                    }
                }
            }
            frame.rax = 0;
        }
        503 => { // sys_poll_event
            if let Some(ev) = pop_user_event() {
                match ev {
                    InputEvent::MouseMove { x, y } => { frame.rax = 1; frame.rdi = ((x as u32) as u64) | (((y as u32) as u64) << 32); },
                    InputEvent::MouseButton { button, pressed } => { frame.rax = 2; frame.rdi = (button as u64) | (if pressed { 1 << 8 } else { 0 }); },
                    InputEvent::KeyDown { keycode, mods } => { frame.rax = 3; frame.rdi = (keycode as u64) | ((mods as u64) << 8); },
                    InputEvent::Char(c) => { frame.rax = 4; frame.rdi = c as u32 as u64; },
                    InputEvent::Scroll { dy } => { frame.rax = 5; frame.rdi = dy as i64 as u64; },
                    _ => { frame.rax = 0; frame.rdi = 0; }
                }
                frame.rsi = 0;
            } else { frame.rax = 0; frame.rdi = 0; frame.rsi = 0; }
        }
        504 => { // sys_present_rects
            let target_w = core::cmp::min(frame.r10 as usize, 1920);
            let target_h = core::cmp::min(frame.r8 as usize, 1080);
            let total_bytes = target_w * target_h * 4;
            let rects_ptr = frame.rsi as *const [u32; 4];
            let rects_len = frame.rdx as usize;

            if target_w > 0 && target_h > 0 && validate_user_range(frame.rdi, total_bytes) && validate_user_range(frame.rsi, rects_len * 16) {
                unsafe {
                    if let Some(writer) = &mut *core::ptr::addr_of_mut!(crate::writer::WRITER) {
                        let src = frame.rdi as *const u32;
                        let dst = writer.buffer as *mut u32;
                        let pitch = writer.pitch / 4;

                        for i in 0..rects_len {
                            let mut rect = [0u32; 4];
                            if copy_from_user(core::slice::from_raw_parts_mut(rect.as_mut_ptr() as *mut u8, 16), rects_ptr as u64 + (i * 16) as u64, 16).is_ok() {
                                let rx = rect[0] as usize;
                                let ry = rect[1] as usize;
                                let rw = rect[2] as usize;
                                let rh = rect[3] as usize;

                                let ex = core::cmp::min(rx + rw, target_w).min(writer.width);
                                let ey = core::cmp::min(ry + rh, target_h).min(writer.height);

                                for y in ry..ey {
                                    let src_idx = y * target_w + rx;
                                    let dst_idx = y * pitch + rx;
                                    let line_len = ex - rx;
                                    core::ptr::copy_nonoverlapping(src.add(src_idx), dst.add(dst_idx), line_len);
                                }
                            }
                        }
                    }
                }
            }
            frame.rax = 0;
        }
        505 => { // sys_wait_event
            let timeout_ms = frame.rdi;
            let start_ticks = crate::arch::x86_64::pit::get_ticks();
            let mut got_event = false;
            loop {
                if let Some(ev) = pop_user_event() {
                    match ev {
                        InputEvent::MouseMove { x, y } => { frame.rax = 1; frame.rdi = ((x as u32) as u64) | (((y as u32) as u64) << 32); },
                        InputEvent::MouseButton { button, pressed } => { frame.rax = 2; frame.rdi = (button as u64) | (if pressed { 1 << 8 } else { 0 }); },
                        InputEvent::KeyDown { keycode, mods } => { frame.rax = 3; frame.rdi = (keycode as u64) | ((mods as u64) << 8); },
                        InputEvent::Char(c) => { frame.rax = 4; frame.rdi = c as u32 as u64; },
                        InputEvent::Scroll { dy } => { frame.rax = 5; frame.rdi = dy as i64 as u64; },
                        _ => { frame.rax = 0; frame.rdi = 0; }
                    }
                    frame.rsi = 0; got_event = true; break;
                }
                if timeout_ms == 0 { break; }
                let elapsed = crate::arch::x86_64::pit::get_ticks().saturating_sub(start_ticks);
                if timeout_ms != u64::MAX && elapsed >= timeout_ms { break; }
                crate::task::yield_now();
            }
            if !got_event { frame.rax = 0; frame.rdi = 0; frame.rsi = 0; }
        }
        512 => { // sys_net_tx
            let data_ptr = frame.rdi;
            let len = frame.rsi as usize;
            if len > 0 && validate_user_range(data_ptr, len) {
                let mut buf = alloc::vec![0u8; len];
                if copy_from_user(&mut buf, data_ptr, len).is_ok() {
                    crate::net::enqueue_tx(buf);
                    frame.rax = len as u64;
                } else { frame.rax = -14i64 as u64; }
            } else { frame.rax = -14i64 as u64; }
        }
        513 => { // sys_net_rx
            let buf_ptr = frame.rdi;
            let max_len = frame.rsi as usize;
            if max_len > 0 && validate_user_range(buf_ptr, max_len) {
                if let Some(packet) = crate::net::dequeue_rx() {
                    let to_copy = core::cmp::min(packet.len(), max_len);
                    if copy_to_user(buf_ptr, &packet[..to_copy]).is_ok() {
                        frame.rax = to_copy as u64;
                    } else { frame.rax = -14i64 as u64; }
                } else {
                    frame.rax = 0;
                }
            } else { frame.rax = -14i64 as u64; }
        }
        514 => { // sys_net_mac
            let buf_ptr = frame.rdi;
            if validate_user_range(buf_ptr, 6) {
                let mac = crate::net::get_mac();
                if copy_to_user(buf_ptr, &mac).is_ok() {
                    frame.rax = 0;
                } else { frame.rax = -14i64 as u64; }
            } else { frame.rax = -14i64 as u64; }
        }
        10 | 13 | 14 | 28 | 131 | 273 | 334 => { frame.rax = 0; }
        39 => { frame.rax = 1; }
        _ => { frame.rax = -38i64 as u64; }
    }
}

pub struct ProcessTableLock {
    pub locked: core::sync::atomic::AtomicBool,
}

impl ProcessTableLock {
    pub const fn new() -> Self { Self { locked: core::sync::atomic::AtomicBool::new(false) } }
    pub fn lock(&self) { while self.locked.compare_exchange_weak(false, true, core::sync::atomic::Ordering::Acquire, core::sync::atomic::Ordering::Relaxed).is_err() { core::hint::spin_loop(); } }
    pub fn unlock(&self) { self.locked.store(false, core::sync::atomic::Ordering::Release); }
}

pub static PROCESS_LOCK: ProcessTableLock = ProcessTableLock::new();

pub static mut PROCESS_TABLE: [Option<ProcessState>; 16] = [const { None }; 16];

pub static CURRENT_PID: [core::sync::atomic::AtomicUsize; 8] = [const { core::sync::atomic::AtomicUsize::new(0) }; 8];

pub fn get_current_process() -> &'static mut ProcessState {
    unsafe {
        let core_id = crate::profiler::get_core_id();
        let pid = CURRENT_PID[core_id].load(core::sync::atomic::Ordering::Acquire);
        if PROCESS_TABLE[pid].is_none() {
            PROCESS_TABLE[pid] = Some(ProcessState {
                mmap_bump: 0x0000_7000_0000_0000,
                fd_table: [const { None }; 64],
            });
        }
        PROCESS_TABLE[pid].as_mut().unwrap()
    }
}

pub fn pop_user_event() -> Option<InputEvent> {
    let tail = USER_EVENT_TAIL.load(Ordering::Acquire);
    let head = USER_EVENT_HEAD.load(Ordering::Acquire);
    if head != tail {
        let ev = unsafe { USERSPACE_EVENTS[tail].take() };
        USER_EVENT_TAIL.store((tail + 1) % 128, Ordering::Release);
        ev
    } else {
        None
    }
}

pub static THREAD_CLEAR_TID: [core::sync::atomic::AtomicU64; 8] = [const { core::sync::atomic::AtomicU64::new(0) }; 8];

pub static THREAD_ACTIVE: [core::sync::atomic::AtomicBool; 8] = [
    core::sync::atomic::AtomicBool::new(false),
    core::sync::atomic::AtomicBool::new(false),
    core::sync::atomic::AtomicBool::new(true),
    core::sync::atomic::AtomicBool::new(false),
    core::sync::atomic::AtomicBool::new(false),
    core::sync::atomic::AtomicBool::new(false),
    core::sync::atomic::AtomicBool::new(false),
    core::sync::atomic::AtomicBool::new(false),
];

pub struct ThreadSpawnContext {
    pub entry: u64,
    pub user_rsp: u64,
    pub tls: u64,
    pub ctid: u64,
    pub cr3: u64,
}

pub static mut THREAD_SPAWN_CONTEXTS: [Option<ThreadSpawnContext>; 8] = [const { None }; 8];

pub fn spawn_thread_on_core(core_id: usize, ctx: ThreadSpawnContext) {
    unsafe {
        THREAD_SPAWN_CONTEXTS[core_id] = Some(ctx);
        THREAD_ACTIVE[core_id].store(true, Ordering::SeqCst);
        let _ = crate::task::dispatch_job(core_id, ap_thread_runner_worker);
    }
}

fn ap_thread_runner_worker() {
    let core_id = crate::profiler::get_core_id();
    let ctx = unsafe { THREAD_SPAWN_CONTEXTS[core_id].take().expect("Missing thread context") };

    let state_ptr = unsafe { core::ptr::addr_of_mut!(CORE_SYSCALL_STATES[core_id]) };
    let spawn_rsp_ptr = unsafe { &mut (*state_ptr).kernel_spawn_rsp as *mut u64 };

    unsafe {
        crate::arch::x86_64::syscall::wrmsr(0xC0000100, ctx.tls);
        if ctx.ctid != 0 {
            THREAD_CLEAR_TID[core_id].store(ctx.ctid, Ordering::SeqCst);
        }

        crate::fs::elf::jump_to_ring3(
            ctx.entry,
            ctx.user_rsp,
            crate::arch::x86_64::gdt::USER_CODE_SELECTOR as u64,
            crate::arch::x86_64::gdt::USER_DATA_SELECTOR as u64,
            spawn_rsp_ptr,
            ctx.cr3,
        );
    }

    let ctid_addr = THREAD_CLEAR_TID[core_id].swap(0, Ordering::SeqCst);
    if ctid_addr != 0 {
        let zero = 0u32;
        let _ = copy_to_user(ctid_addr, &zero.to_ne_bytes());
    }
    THREAD_ACTIVE[core_id].store(false, Ordering::SeqCst);
}
