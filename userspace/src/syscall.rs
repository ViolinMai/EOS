use core::arch::asm;

pub const SEEK_SET: u64 = 0;
pub const SEEK_CUR: u64 = 1;
pub const SEEK_END: u64 = 2;

#[repr(C)]
pub struct TimeSpec {
    pub tv_sec: u64,
    pub tv_nsec: u64,
}

pub fn sys_exit(code: u64) -> ! {
    unsafe {
        asm!(
            "syscall",
            in("rax") 1,
            in("rdi") code,
            options(noreturn)
        );
    }
}

pub fn sys_write(fd: u64, buf: &[u8]) -> i64 {
    let ret: i64;
    unsafe {
        asm!(
            "syscall",
            in("rax") 2,
            in("rdi") fd,
            in("rsi") buf.as_ptr() as u64,
            in("rdx") buf.len() as u64,
            out("rcx") _, out("r11") _,
            lateout("rax") ret,
        );
    }
    ret
}

pub fn sys_read(fd: u64, buf: &mut [u8]) -> i64 {
    let ret: i64;
    unsafe {
        asm!(
            "syscall",
            in("rax") 3,
            in("rdi") fd,
            in("rsi") buf.as_mut_ptr() as u64,
            in("rdx") buf.len() as u64,
            out("rcx") _, out("r11") _,
            lateout("rax") ret,
        );
    }
    ret
}

pub fn sys_open(path: &str) -> i64 {
    let mut path_buf = alloc::vec::Vec::with_capacity(path.len() + 1);
    path_buf.extend_from_slice(path.as_bytes());
    path_buf.push(0);

    let ret: i64;
    unsafe {
        asm!(
            "syscall",
            in("rax") 4,
            in("rdi") path_buf.as_ptr() as u64,
            in("rsi") 0,
            out("rcx") _, out("r11") _,
            lateout("rax") ret,
        );
    }
    ret
}

pub fn sys_close(fd: u64) -> i64 {
    let ret: i64;
    unsafe {
        asm!(
            "syscall",
            in("rax") 5,
            in("rdi") fd,
            out("rcx") _, out("r11") _,
            lateout("rax") ret,
        );
    }
    ret
}

pub fn sys_lseek(fd: u64, offset: i64, whence: u64) -> i64 {
    let ret: i64;
    unsafe {
        asm!(
            "syscall",
            in("rax") 8,
            in("rdi") fd,
            in("rsi") offset,
            in("rdx") whence,
            out("rcx") _, out("r11") _,
            lateout("rax") ret,
        );
    }
    ret
}

pub fn sys_mmap(size: usize) -> *mut u8 {
    let ret: u64;
    unsafe {
        asm!(
            "syscall",
            in("rax") 9,
            in("rdi") size as u64,
            out("rcx") _, out("r11") _,
            lateout("rax") ret,
        );
    }
    ret as *mut u8
}

pub fn sys_sleep(ms: u64) {
    unsafe {
        asm!("syscall", in("rax") 100, in("rdi") ms, out("rcx") _, out("r11") _);
    }
}

pub fn sys_clear_screen() {
    unsafe {
        asm!("syscall", in("rax") 101, out("rcx") _, out("r11") _);
    }
}

pub fn sys_blit_image_ptr(ptr: *const u32, x: usize, y: usize, w: usize, h: usize) {
    unsafe {
        asm!(
            "syscall",
            in("rax") 102,
            in("rdi") ptr as u64,
            in("rsi") x as u64,
            in("rdx") y as u64,
            in("r10") w as u64,
            in("r8") h as u64,
            out("rcx") _, out("r11") _
        );
    }
}

pub fn sys_clock_gettime(clock_id: u64, tp: &mut TimeSpec) -> i64 {
    let ret: i64;
    unsafe {
        asm!(
            "syscall",
            in("rax") 228,
            in("rdi") clock_id,
            in("rsi") tp as *mut _ as u64,
            out("rcx") _, out("r11") _,
            lateout("rax") ret,
        );
    }
    ret
}

pub fn print_str(s: &str) {
    sys_write(1, s.as_bytes());
}

pub fn print_num(mut n: usize) {
    if n == 0 {
        print_str("0");
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 20;
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    sys_write(1, &buf[i..20]);
}
