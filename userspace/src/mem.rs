#![allow(suspicious_runtime_symbol_definitions)]

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    core::arch::asm!(
        "rep movsb",
        inout("rdi") dest => _,
        inout("rsi") src => _,
        inout("rcx") n => _,
        options(nostack, preserves_flags)
    );
    dest
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memset(dest: *mut u8, c: i32, n: usize) -> *mut u8 {
    core::arch::asm!(
        "rep stosb",
        inout("rdi") dest => _,
        in("al") c as u8,
        inout("rcx") n => _,
        options(nostack, preserves_flags)
    );
    dest
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memmove(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    if (dest as usize) <= (src as usize) || (dest as usize) >= (src as usize + n) {
        memcpy(dest, src, n)
    } else {
        core::arch::asm!(
            "std",
            "rep movsb",
            "cld",
            inout("rdi") dest.add(n - 1) => _,
            inout("rsi") src.add(n - 1) => _,
            inout("rcx") n => _,
            options(nostack, preserves_flags)
        );
        dest
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcmp(s1: *const u8, s2: *const u8, n: usize) -> i32 {
    for i in 0..n {
        let a = *s1.add(i);
        let b = *s2.add(i);
        if a != b {
            return a as i32 - b as i32;
        }
    }
    0
}
