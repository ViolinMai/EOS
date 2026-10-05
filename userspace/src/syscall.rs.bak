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

pub fn sys_present_rects(pixels: *const u32, width: usize, height: usize, rects: &[[u32; 4]]) {
    unsafe {
        core::arch::asm!("syscall", in("rax") 504, in("rdi") pixels as u64, in("rsi") rects.as_ptr() as u64, in("rdx") rects.len() as u64, in("r10") width as u64, in("r8") height as u64, out("rcx") _, out("r11") _);
    }
}

pub fn sys_net_tx(data: &[u8]) {
    unsafe {
        core::arch::asm!("syscall", in("rax") 512, in("rdi") data.as_ptr() as u64, in("rsi") data.len() as u64, out("rcx") _, out("r11") _);
    }
}

pub fn sys_net_rx(buf: &mut [u8]) -> usize {
    let len: usize;
    unsafe {
        core::arch::asm!("syscall", inout("rax") 513u64 => len, in("rdi") buf.as_mut_ptr() as u64, in("rsi") buf.len() as u64, out("rcx") _, out("r11") _);
    }
    len
}

pub fn sys_net_mac(buf: &mut [u8; 6]) {
    unsafe {
        core::arch::asm!("syscall", in("rax") 514, in("rdi") buf.as_mut_ptr() as u64, out("rcx") _, out("r11") _);
    }
}
