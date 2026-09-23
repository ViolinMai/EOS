use crate::serial_print;
use crate::writer::WRITER;
use core::fmt;

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

pub struct KernelLogger;

impl KernelLogger {
    pub fn log(level: LogLevel, args: fmt::Arguments) {
        // 1. الخروج الفوري عبر المنفذ التسلسلي (Serial)
        let (tag, color_prefix) = match level {
            LogLevel::Debug => ("[DEBUG]", "\x1b[36m"),
            LogLevel::Info  => ("[INFO] ", "\x1b[32m"),
            LogLevel::Warn  => ("[WARN] ", "\x1b[33m"),
            LogLevel::Error => ("[ERROR]", "\x1b[31m"),
        };

        serial_print!("{}{}{} {}\x1b[0m\n", color_prefix, tag, "\x1b[0m", args);

        // 2. العرض المتزامن على الشاشة (Framebuffer Console)
        let (r, g, b) = match level {
            LogLevel::Debug => (148, 163, 184), // رمادي فاتح
            LogLevel::Info  => (74, 222, 128),  // أخضر زمردي
            LogLevel::Warn  => (250, 204, 21),  // أصفر تحذيري
            LogLevel::Error => (248, 113, 113), // أحمر فاقع
        };

        unsafe {
            if let Some(writer) = &mut *core::ptr::addr_of_mut!(WRITER) {
                writer.write_str(tag, r, g, b);
                writer.write_char(' ', 255, 255, 255);
                writer.write_fmt(args, 241, 245, 249);
                writer.write_char('\n', 255, 255, 255);
            }
        }
    }
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::logger::KernelLogger::log($crate::logger::LogLevel::Info, format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        $crate::logger::KernelLogger::log($crate::logger::LogLevel::Warn, format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        $crate::logger::KernelLogger::log($crate::logger::LogLevel::Error, format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        $crate::logger::KernelLogger::log($crate::logger::LogLevel::Debug, format_args!($($arg)*))
    };
}
