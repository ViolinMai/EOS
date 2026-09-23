use core::arch::{asm, naked_asm};
use crate::arch::x86_64::keyboard::pop_char_from_buffer;
use crate::arch::x86_64::pit;
use crate::fs::vfs_read_bytes;
use crate::log_info;
use crate::writer::WRITER;
use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicBool, Ordering};

const IA32_EFER: u32 = 0xC0000080;
const IA32_STAR: u32 = 0xC0000081;
const IA32_LSTAR: u32 = 0xC0000082;
const IA32_FMASK: u32 = 0xC0000084;

pub static ELF_EXIT_REQUESTED: AtomicBool = AtomicBool::new(false);
pub static mut KERNEL_SAVED_RSP: u64 = 0;
pub static mut USER_TEMP_RSP: u64 = 0;

#[allow(dead_code)]
#[repr(align(4096))]
struct KernelSyscallStack([u8; 16384]);
static mut SYSCALL_KERNEL_STACK: KernelSyscallStack = KernelSyscallStack([0; 16384]);

#[inline]
unsafe fn wrmsr(msr: u32, val: u64) {
    let lo = (val & 0xFFFF_FFFF) as u32;
    let hi = (val >> 32) as u32;
    unsafe {
        asm!("wrmsr", in("ecx") msr, in("eax") lo, in("edx") hi, options(nomem, nostack, preserves_flags));
    }
}

#[inline]
unsafe fn rdmsr(msr: u32) -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        asm!("rdmsr", in("ecx") msr, out("eax") lo, out("edx") hi, options(nomem, nostack, preserves_flags));
    }
    ((hi as u64) << 32) | (lo as u64)
}

pub fn init() {
    unsafe {
        let efer = rdmsr(IA32_EFER);
        wrmsr(IA32_EFER, efer | 1);

        let star = ((0x13u64) << 48) | ((0x08u64) << 32);
        wrmsr(IA32_STAR, star);

        wrmsr(IA32_LSTAR, syscall_entry as *const () as usize as u64);

        wrmsr(IA32_FMASK, 1 << 9);

        log_info!("SYSCALL", "Extended Graphics Interface (BLIT_BUFFER) & VFS ready.");
    }
}

// 💡 توسيع Syscall Dispatcher ليقبل المعامل الرابع (arg4) لتأمين الـ Bounds
#[unsafe(no_mangle)]
extern "C" fn syscall_dispatcher(id: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64) -> u64 {
    match id {
        0 => {
            let buf_ptr = arg2 as *mut u8;
            let count = arg3 as usize;
            if buf_ptr.is_null() || count == 0 {
                return 0;
            }

            let mut read_bytes = 0usize;
            unsafe {
                while read_bytes < count {
                    if !crate::arch::x86_64::interrupts::IS_RUNNING_PROGRAM.load(Ordering::Relaxed) {
                        ELF_EXIT_REQUESTED.store(true, Ordering::SeqCst);
                        break;
                    }

                    if let Some(byte) = pop_char_from_buffer() {
                        *buf_ptr.add(read_bytes) = byte;
                        read_bytes += 1;

                        if let Some(w) = &mut *addr_of_mut!(WRITER) {
                            w.write_char(byte as char, 248, 250, 252);
                        }
                        crate::serial_print!("{}", byte as char);

                        if byte == b'\n' || byte == b'\r' {
                            break;
                        }
                    } else {
                        asm!("sti; hlt", options(nomem, nostack));
                    }
                }
            }
            read_bytes as u64
        }
        1 => {
            unsafe {
                let slice = core::slice::from_raw_parts(arg2 as *const u8, arg3 as usize);
                if let Ok(s) = core::str::from_utf8(slice) {
                    if let Some(w) = &mut *addr_of_mut!(WRITER) {
                        w.write_str(s, 248, 250, 252);
                    }
                    crate::serial_print!("{}", s);
                }
            }
            arg3
        }
        2 => pit::get_uptime_seconds(),

        10 => {
            let buf_ptr = arg1 as *const u32;
            let width = (arg2 >> 32) as usize;
            let height = (arg2 & 0xFFFF_FFFF) as usize;
            let x = (arg3 >> 32) as usize;
            let y = (arg3 & 0xFFFF_FFFF) as usize;

            if buf_ptr.is_null() || width == 0 || height == 0 || width > 2048 || height > 2048 {
                return 0xFFFF_FFFF_FFFF_FFFF;
            }

            unsafe {
                let total_pixels = width * height;
                let slice = core::slice::from_raw_parts(buf_ptr, total_pixels);
                if let Some(w) = &mut *addr_of_mut!(WRITER) {
                    w.blit_buffer_alpha(slice, x, y, width, height);
                }
            }
            0
        }

        11 => {
            unsafe {
                if let Some(w) = &mut *addr_of_mut!(WRITER) {
                    w.clear(15, 23, 42);
                }
            }
            0
        }

        20 => {
            let name_ptr = arg1 as *const u8;
            let name_len = arg2 as usize;
            let dest_ptr = arg3 as *mut u8;
            let dest_max_len = arg4 as usize;

            if name_ptr.is_null() || dest_ptr.is_null() || name_len == 0 {
                return 0xFFFF_FFFF_FFFF_FFFF;
            }

            unsafe {
                let name_slice = core::slice::from_raw_parts(name_ptr, name_len);
                if let Ok(path) = core::str::from_utf8(name_slice) {
                    if let Ok(file_bytes) = vfs_read_bytes(path) {
                        if file_bytes.len() > dest_max_len {
                            crate::log_error!("SYSCALL", "sys_read_file: User buffer too small ({} < {})", dest_max_len, file_bytes.len());
                            return 0xFFFF_FFFF_FFFF_FFFF;
                        }
                        core::ptr::copy_nonoverlapping(file_bytes.as_ptr(), dest_ptr, file_bytes.len());
                        return file_bytes.len() as u64;
                    }
                }
            }
            0xFFFF_FFFF_FFFF_FFFF
        }

        35 => {
            let ms = arg1;
            unsafe {
                asm!("sti", options(nomem, nostack));
            }
            pit::sleep_ms(ms);
            0
        }
        39 => 1001,
        60 => {
            log_info!("SYSCALL", "Process exited with status code {}.", arg1);
            ELF_EXIT_REQUESTED.store(true, Ordering::SeqCst);
            0
        }
        _ => {
            log_info!("SYSCALL", "Unknown ID {}", id);
            0xFFFF_FFFF_FFFF_FFFF
        }
    }
}

// 💡 تعديل ربط مسجلات معمارية x86_64 لتدعم حتى 4 معاملات (arg4 في مسجل r10)
#[unsafe(naked)]
extern "C" fn syscall_entry() {
    naked_asm!(
        "mov [{user_rsp_temp}], rsp",
        "lea rsp, [{kstack} + 16380]",
        "push qword ptr [{user_rsp_temp}]",
        "push rcx",
        "push r11",
        "push rbx",
        "push rbp",
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        "mov r8, r10",
        "mov rcx, rdx",
        "mov rdx, rsi",
        "mov rsi, rdi",
        "mov rdi, rax",
        "call {dispatcher}",

        "push rax",

        "cmp qword ptr [{exit_flag}], 0",
        "jne 2f",

        "pop rax",

        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbp",
        "pop rbx",
        "pop r11",
        "pop rcx",
        "pop rsp",
        "sysretq",

        "2:",
        "pop rax",
        "mov rsp, [{kernel_sp}]",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        "sti",
        "ret",

        dispatcher = sym syscall_dispatcher,
        exit_flag = sym ELF_EXIT_REQUESTED,
        kernel_sp = sym KERNEL_SAVED_RSP,
        user_rsp_temp = sym USER_TEMP_RSP,
        kstack = sym SYSCALL_KERNEL_STACK,
    );
}
