use crate::serial_print;
use core::fmt;
use core::sync::atomic::{AtomicUsize, AtomicBool, Ordering};

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum LogLevel { Debug, Info, Warn, Error, Fatal }

pub struct LogEvent {
    pub level: LogLevel,
    pub timestamp: u64,
    pub message: [u8; 128],
    pub len: usize,
}

const LOG_CAPACITY: usize = 1024;
static mut LOG_QUEUE: [Option<LogEvent>; LOG_CAPACITY] = [const { None }; LOG_CAPACITY];
static LOG_HEAD: AtomicUsize = AtomicUsize::new(0);
static LOG_TAIL: AtomicUsize = AtomicUsize::new(0);
static WRITE_LOCK: AtomicBool = AtomicBool::new(false);

pub struct KernelLogger;

impl KernelLogger {
    pub fn log(level: LogLevel, subsystem: &'static str, args: fmt::Arguments) {
        let mut msg_buf = [0u8; 128];
        let mut formatter = MessageBuffer { buf: &mut msg_buf, cursor: 0 };
        let _ = core::fmt::write(&mut formatter, format_args!("[{}] {}", subsystem, args));
        
        let len = formatter.cursor;
        let event = LogEvent {
            level,
            timestamp: crate::arch::x86_64::pit::get_ticks(),
            message: msg_buf,
            len,
        };

        let head = LOG_HEAD.load(Ordering::Relaxed);
        let next_head = (head + 1) % LOG_CAPACITY;
        unsafe { LOG_QUEUE[head] = Some(event); }
        LOG_HEAD.store(next_head, Ordering::Release);

        if WRITE_LOCK.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
            Self::flush_to_serial();
            WRITE_LOCK.store(false, Ordering::Release);
        }
    }

    pub fn flush_to_serial() {
        let mut tail = LOG_TAIL.load(Ordering::Acquire);
        let head = LOG_HEAD.load(Ordering::Acquire);
        while tail != head {
            if let Some(event) = unsafe { &LOG_QUEUE[tail] } {
                let color = match event.level {
                    LogLevel::Debug => "\x1b[36m",
                    LogLevel::Info  => "\x1b[32m",
                    LogLevel::Warn  => "\x1b[33m",
                    LogLevel::Error => "\x1b[31m",
                    LogLevel::Fatal => "\x1b[35m",
                };
                let sec = event.timestamp / 100;
                let frac = (event.timestamp % 100) * 10;
                
                if let Ok(s) = core::str::from_utf8(&event.message[..event.len]) {
                    serial_print!("{}\x1b[1;30m[{:03}.{:02}s]\x1b[0m {}\n", color, sec, frac, s);
                }
            }
            tail = (tail + 1) % LOG_CAPACITY;
        }
        LOG_TAIL.store(tail, Ordering::Release);
    }
}

struct MessageBuffer<'a> { buf: &'a mut [u8], cursor: usize }
impl<'a> fmt::Write for MessageBuffer<'a> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let remaining = self.buf.len() - self.cursor;
        let to_copy = core::cmp::min(remaining, s.len());
        self.buf[self.cursor..self.cursor + to_copy].copy_from_slice(&s.as_bytes()[..to_copy]);
        self.cursor += to_copy;
        Ok(())
    }
}

#[macro_export]
macro_rules! log_info {
    ($subsys:literal, $fmt:literal $(, $($arg:tt)*)?) => {
        $crate::logger::KernelLogger::log($crate::logger::LogLevel::Info, $subsys, format_args!($fmt $(, $($arg)*)?))
    };
    ($fmt:literal $(, $($arg:tt)*)?) => {
        $crate::logger::KernelLogger::log($crate::logger::LogLevel::Info, "KERNEL", format_args!($fmt $(, $($arg)*)?))
    };
}
#[macro_export]
macro_rules! log_warn {
    ($subsys:literal, $fmt:literal $(, $($arg:tt)*)?) => {
        $crate::logger::KernelLogger::log($crate::logger::LogLevel::Warn, $subsys, format_args!($fmt $(, $($arg)*)?))
    };
    ($fmt:literal $(, $($arg:tt)*)?) => {
        $crate::logger::KernelLogger::log($crate::logger::LogLevel::Warn, "KERNEL", format_args!($fmt $(, $($arg)*)?))
    };
}
#[macro_export]
macro_rules! log_error {
    ($subsys:literal, $fmt:literal $(, $($arg:tt)*)?) => {
        $crate::logger::KernelLogger::log($crate::logger::LogLevel::Error, $subsys, format_args!($fmt $(, $($arg)*)?))
    };
    ($fmt:literal $(, $($arg:tt)*)?) => {
        $crate::logger::KernelLogger::log($crate::logger::LogLevel::Error, "KERNEL", format_args!($fmt $(, $($arg)*)?))
    };
}
#[macro_export]
macro_rules! log_debug {
    ($subsys:literal, $fmt:literal $(, $($arg:tt)*)?) => {
        $crate::logger::KernelLogger::log($crate::logger::LogLevel::Debug, $subsys, format_args!($fmt $(, $($arg)*)?))
    };
    ($fmt:literal $(, $($arg:tt)*)?) => {
        $crate::logger::KernelLogger::log($crate::logger::LogLevel::Debug, "KERNEL", format_args!($fmt $(, $($arg)*)?))
    };
}
#[macro_export]
macro_rules! log_fatal {
    ($subsys:literal, $fmt:literal $(, $($arg:tt)*)?) => {
        $crate::logger::KernelLogger::log($crate::logger::LogLevel::Fatal, $subsys, format_args!($fmt $(, $($arg)*)?))
    };
    ($fmt:literal $(, $($arg:tt)*)?) => {
        $crate::logger::KernelLogger::log($crate::logger::LogLevel::Fatal, "KERNEL", format_args!($fmt $(, $($arg)*)?))
    };
}
