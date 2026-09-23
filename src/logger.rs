use crate::serial_print;
use crate::writer::WRITER;
use core::fmt;
use core::sync::atomic::{AtomicBool, Ordering};

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

pub struct KernelLogger;

static PRINT_LOCK: AtomicBool = AtomicBool::new(false);

#[inline]
fn get_current_core_id() -> u32 {
    let ebx_val: u32;
    unsafe {
        // 💡 حفظ rbx واستخراج قيمته دون حجزه كـ operand مباشر لـ LLVM
        core::arch::asm!(
            "push rbx",
            "mov eax, 1",
            "cpuid",
            "mov {0:e}, ebx",
            "pop rbx",
            out(reg) ebx_val,
            out("eax") _,
            out("ecx") _,
            out("edx") _,
            options(nomem, preserves_flags)
        );
    }
    (ebx_val >> 24) & 0xFF
}

impl KernelLogger {
    pub fn log(level: LogLevel, subsystem: &'static str, args: fmt::Arguments) {
        let (tag, color_prefix) = match level {
            LogLevel::Debug => ("[DEBUG]", "\x1b[36m"),
            LogLevel::Info  => ("[INFO] ", "\x1b[32m"),
            LogLevel::Warn  => ("[WARN] ", "\x1b[33m"),
            LogLevel::Error => ("[ERROR]", "\x1b[31m"),
            LogLevel::Fatal => ("[FATAL]", "\x1b[35m"),
        };

        while PRINT_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }

        let core_id = get_current_core_id();

        serial_print!("{}{}\x1b[0m \x1b[1;34m[CPU#{}]\x1b[0m \x1b[1;30m[{:<6}]\x1b[0m {}\n",
            color_prefix, tag, core_id, subsystem, args
        );

        let (r, g, b) = match level {
            LogLevel::Debug => (148, 163, 184),
            LogLevel::Info  => (74, 222, 128),
            LogLevel::Warn  => (250, 204, 21),
            LogLevel::Error => (248, 113, 113),
            LogLevel::Fatal => (236, 72, 153),
        };

        unsafe {
            if let Some(writer) = &mut *core::ptr::addr_of_mut!(WRITER) {
                writer.write_str(tag, r, g, b);
                writer.write_str(" [C", 100, 116, 139);
                let c_char = (b'0' + (core_id as u8 % 10)) as char;
                writer.write_char(c_char, 56, 189, 248);
                writer.write_str("] [", 100, 116, 139);
                writer.write_str(subsystem, 148, 163, 184);
                writer.write_str("] ", 100, 116, 139);
                writer.write_fmt(args, 241, 245, 249);
                writer.write_char('\n', 255, 255, 255);
            }
        }

        PRINT_LOCK.store(false, Ordering::Release);
    }
}

#[macro_export]
macro_rules! log_info {
    ($subsys:literal, $fmt:literal $(, $($arg:tt)*)?) => {
        $crate::logger::KernelLogger::log(
            $crate::logger::LogLevel::Info,
            $subsys,
            format_args!($fmt $(, $($arg)*)?)
        )
    };
    ($fmt:literal $(, $($arg:tt)*)?) => {
        $crate::logger::KernelLogger::log(
            $crate::logger::LogLevel::Info,
            "KERNEL",
            format_args!($fmt $(, $($arg)*)?)
        )
    };
}

#[macro_export]
macro_rules! log_warn {
    ($subsys:literal, $fmt:literal $(, $($arg:tt)*)?) => {
        $crate::logger::KernelLogger::log(
            $crate::logger::LogLevel::Warn,
            $subsys,
            format_args!($fmt $(, $($arg)*)?)
        )
    };
    ($fmt:literal $(, $($arg:tt)*)?) => {
        $crate::logger::KernelLogger::log(
            $crate::logger::LogLevel::Warn,
            "KERNEL",
            format_args!($fmt $(, $($arg)*)?)
        )
    };
}

#[macro_export]
macro_rules! log_error {
    ($subsys:literal, $fmt:literal $(, $($arg:tt)*)?) => {
        $crate::logger::KernelLogger::log(
            $crate::logger::LogLevel::Error,
            $subsys,
            format_args!($fmt $(, $($arg)*)?)
        )
    };
    ($fmt:literal $(, $($arg:tt)*)?) => {
        $crate::logger::KernelLogger::log(
            $crate::logger::LogLevel::Error,
            "KERNEL",
            format_args!($fmt $(, $($arg)*)?)
        )
    };
}

#[macro_export]
macro_rules! log_debug {
    ($subsys:literal, $fmt:literal $(, $($arg:tt)*)?) => {
        $crate::logger::KernelLogger::log(
            $crate::logger::LogLevel::Debug,
            $subsys,
            format_args!($fmt $(, $($arg)*)?)
        )
    };
    ($fmt:literal $(, $($arg:tt)*)?) => {
        $crate::logger::KernelLogger::log(
            $crate::logger::LogLevel::Debug,
            "KERNEL",
            format_args!($fmt $(, $($arg)*)?)
        )
    };
}

#[macro_export]
macro_rules! log_fatal {
    ($subsys:literal, $fmt:literal $(, $($arg:tt)*)?) => {
        $crate::logger::KernelLogger::log(
            $crate::logger::LogLevel::Fatal,
            $subsys,
            format_args!($fmt $(, $($arg)*)?)
        )
    };
    ($fmt:literal $(, $($arg:tt)*)?) => {
        $crate::logger::KernelLogger::log(
            $crate::logger::LogLevel::Fatal,
            "KERNEL",
            format_args!($fmt $(, $($arg)*)?)
        )
    };
}
