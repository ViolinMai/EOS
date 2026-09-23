use core::arch::naked_asm;

// تبديل سياق المعالج عتادياً:
// المعامل الأول (rdi) = مؤشر لحفظ rsp المهمة الحالية (*mut u64)
// المعامل الثاني (rsi) = قيمة rsp للمهمة الجديدة (u64)
#[unsafe(no_mangle)]
#[unsafe(naked)]
pub unsafe extern "C" fn context_switch(prev_rsp: *mut u64, next_rsp: u64) {
    naked_asm!(
        // 1. حفظ مسجلات الـ Callee-saved للـ System V AMD64 ABI
        "push rbp",
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        // 2. حفظ الـ RSP الحالي في هيكل المهمة القديمة
        "mov [rdi], rsp",

        // 3. التبديل لمكدس المهمة الجديدة
        "mov rsp, rsi",

        // 4. استرجاع مسجلات المهمة الجديدة
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",

        // 5. العودة (ret) إلى RIP المحفوظ على قمة المكدس الجديد
        "ret"
    );
}
