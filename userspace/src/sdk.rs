#![allow(dead_code)]
pub mod window {
    use core::arch::asm;

    pub fn present(pixels: *const u32, width: usize, height: usize) {
        unsafe {
            asm!(
                "syscall",
                in("rax") 502,
                in("rdi") pixels as u64,
                in("rsi") 0,
                in("rdx") 0,
                in("r10") width as u64,
                in("r8") height as u64,
                out("rcx") _, out("r11") _
            );
        }
    }

    pub fn present_rects(pixels: *const u32, width: usize, height: usize, rects: &[[u32; 4]]) {
        unsafe {
            asm!("syscall", in("rax") 504, in("rdi") pixels as u64, in("rsi") rects.as_ptr() as u64, in("rdx") rects.len() as u64, in("r10") width as u64, in("r8") height as u64, out("rcx") _, out("r11") _);
        }
    }

    pub fn poll_event() -> Option<[u64; 3]> {
        let type_val: u64;
        let data1: u64;
        let data2: u64;
        unsafe {
            asm!(
                "syscall",
                inout("rax") 503u64 => type_val,
                out("rdi") data1,
                out("rsi") data2,
                out("rcx") _, out("r11") _
            );
        }
        if type_val == 0 { None } else { Some([type_val, data1, data2]) }
    }

    pub fn wait_event(timeout_ms: u64) -> Option<[u64; 3]> {
        let type_val: u64;
        let data1: u64;
        let data2: u64;
        unsafe {
            asm!(
                "syscall",
                inout("rax") 505u64 => type_val,
                inout("rdi") timeout_ms => data1,
                out("rsi") data2,
                out("rcx") _, out("r11") _
            );
        }
        if type_val == 0 { None } else { Some([type_val, data1, data2]) }
    }
}

pub mod gfx {
    #[inline(always)]
    pub fn blend(bg: u32, fg: u32, alpha: u32) -> u32 {
        if alpha >= 255 { return fg | 0xFF000000; }
        if alpha == 0 { return bg | 0xFF000000; }
        let inv = 255 - alpha;
        let rb = (((fg & 0x00FF00FF) * alpha + (bg & 0x00FF00FF) * inv) >> 8) & 0x00FF00FF;
        let g = (((fg & 0x0000FF00) * alpha + (bg & 0x0000FF00) * inv) >> 8) & 0x0000FF00;
        0xFF000000 | rb | g
    }
}
