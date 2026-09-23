use core::arch::asm;

// 🔧 تأمين واجهة الـ Syscalls لـ Userspace لمنع الـ ABI Mismatch
#[inline(never)]
pub fn sys_read(fd: u64, buf: &mut [u8]) -> usize {
    let ret: u64;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") 0u64 => ret,
            in("rdi") fd,
            in("rsi") buf.as_mut_ptr() as u64,
            in("rdx") buf.len() as u64,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret as usize
}

#[inline(never)]
pub fn sys_write(fd: u64, buf: &[u8]) -> usize {
    let ret: u64;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") 1u64 => ret,
            in("rdi") fd,
            in("rsi") buf.as_ptr() as u64,
            in("rdx") buf.len() as u64,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret as usize
}

#[inline(never)]
pub fn sys_blit_image_ptr(ptr: *const u32, x: usize, y: usize, w: usize, h: usize) -> u64 {
    let ret: u64;
    let dims = ((w as u64) << 32) | (h as u64);
    let coords = ((x as u64) << 32) | (y as u64);
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") 10u64 => ret,
            in("rdi") ptr as u64,
            in("rsi") dims,
            in("rdx") coords,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

#[inline(never)]
pub fn sys_read_file(filename: &str, dest: &mut [u8]) -> isize {
    let ret: i64;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") 20u64 => ret,
            in("rdi") filename.as_ptr() as u64,
            in("rsi") filename.len() as u64,
            in("rdx") dest.as_mut_ptr() as u64,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret as isize
}

#[inline(never)]
pub fn sys_sleep(ms: u64) {
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") 35u64 => _,
            in("rdi") ms,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
}

#[inline(never)]
pub fn sys_exit(code: u64) -> ! {
    unsafe {
        asm!(
            "syscall",
            in("rax") 60u64,
            in("rdi") code,
            options(noreturn)
        );
    }
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
    let mut i = 0;
    while n > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    for j in 0..i / 2 {
        buf.swap(j, i - 1 - j);
    }
    if let Ok(s) = core::str::from_utf8(&buf[..i]) {
        print_str(s);
    }
}
