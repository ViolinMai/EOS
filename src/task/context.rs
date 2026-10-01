use core::arch::naked_asm;

#[unsafe(no_mangle)]
#[unsafe(naked)]
pub unsafe extern "C" fn context_switch(prev_rsp: *mut u64, next_rsp: u64) {
    naked_asm!(
        "push rbp", "push rbx", "push r12", "push r13", "push r14", "push r15",
        "sub rsp, 512",
        "and rsp, -16",
        "fxsave [rsp]",
        "mov [rdi], rsp",
        "mov rsp, rsi",
        "fxrstor [rsp]",
        "add rsp, 512",
        "pop r15", "pop r14", "pop r13", "pop r12", "pop rbx", "pop rbp",
        "ret"
    );
}
