#![allow(dead_code)]
use core::arch::asm;

pub fn sys_present(pixels: *const u32, w: usize, h: usize) {
    unsafe {
        asm!("syscall", in("rax") 502, in("rdi") pixels as u64, in("rsi") 0, in("rdx") 0, in("r10") w as u64, in("r8") h as u64, out("rcx") _, out("r11") _);
    }
}

pub fn sys_poll_event() -> Option<[u64; 2]> {
    let type_val: u64; let data_val: u64;
    unsafe {
        asm!("syscall", inout("rax") 503u64 => type_val, out("rdi") data_val, out("rcx") _, out("r11") _);
    }
    if type_val == 0 { None } else { Some([type_val, data_val]) }
}
