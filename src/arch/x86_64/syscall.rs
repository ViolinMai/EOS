use core::arch::{asm, naked_asm};
use crate::arch::x86_64::keyboard::pop_char_from_buffer;
use crate::arch::x86_64::pit;
use crate::fs::tar::INITRD;
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

        log_info!("Syscall: Extended Graphics Interface (BLIT_BUFFER) ready.");
    }
}

#[unsafe(no_mangle)]
extern "C" fn syscall_dispatcher(id: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
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

        // SYS_BLIT_BUFFER: arg1 = *const u32, arg2 = (w << 32) | h, arg3 = (x << 32) | y
        10 => {
            let buf_ptr = arg1 as *const u32;
            let width = (arg2 >> 32) as usize;
            let height = (arg2 & 0xFFFF_FFFF) as usize;
            let x = (arg3 >> 32) as usize;
            let y = (arg3 & 0xFFFF_FFFF) as usize;

            if buf_ptr.is_null() || width == 0 || height == 0 {
                return 0xFFFF_FFFF_FFFF_FFFF;
            }

            unsafe {
                let slice = core::slice::from_raw_parts(buf_ptr, width * height);
                if let Some(w) = &mut *addr_of_mut!(WRITER) {
                    w.blit_buffer_alpha(slice, x, y, width, height);
                }
            }
            0
        }

        // SYS_READ_FILE: arg1 = *const u8 (filename), arg2 = len, arg3 = *mut u8 (dest_buf)
        20 => {
            let name_ptr = arg1 as *const u8;
            let name_len = arg2 as usize;
            let dest_ptr = arg3 as *mut u8;

            if name_ptr.is_null() || dest_ptr.is_null() || name_len == 0 {
                return 0xFFFF_FFFF_FFFF_FFFF;
            }

            unsafe {
                let name_slice = core::slice::from_raw_parts(name_ptr, name_len);
                if let Ok(raw_name_str) = core::str::from_utf8(name_slice) {
                    let target_name = raw_name_str.trim().strip_prefix("./").unwrap_or(raw_name_str.trim());
                    if let Some(archive) = &*addr_of_mut!(INITRD) {
                        if let Some(file) = archive.files.iter().find(|f| f.name == target_name) {
                            core::ptr::copy_nonoverlapping(file.data_ptr, dest_ptr, file.size);
                            return file.size as u64;
                        }
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
            log_info!("Syscall: Process exited with status code {}.", arg1);
            ELF_EXIT_REQUESTED.store(true, Ordering::SeqCst);
            0
        }
        _ => {
            log_info!("Syscall: Unknown ID {}", id);
            0xFFFF_FFFF_FFFF_FFFF
        }
    }
}

// 🔧 إصلاح الكارثة: الحفاظ على مسجل RBX بالكامل!
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

        "mov rcx, rdx",
        "mov rdx, rsi",
        "mov rsi, rdi",
        "mov rdi, rax",
        "call {dispatcher}",

        "push rax", // حفظ قيمة الاسترجاع مؤقتاً لتفادي تدمير مسجلات المستخدم

        "cmp qword ptr [{exit_flag}], 0",
        "jne 2f",

        "pop rax",  // استعادة قيمة الاسترجاع لترسل للمستخدم

        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbp",
        "pop rbx",  // استرجاع RBX المستخدم بدقة
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
        "pop rbx",  // استرجاع RBX المستخدم بدقة قبل الخروج للكيرنل
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

pub fn trigger_test_syscall(msg: &str) -> u64 {
    let id: u64 = 1;
    let ptr = msg.as_ptr() as u64;
    let len = msg.len() as u64;
    let ret: u64;

    unsafe {
        asm!(
            "syscall",
            inlateout("rax") id => ret,
            in("rdi") 1u64,
            in("rsi") ptr,
            in("rdx") len,
            out("rcx") _,
            out("r11") _,
            options(nostack)
        );
    }
    ret
}
