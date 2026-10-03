use std::time::SystemTime;

pub fn log_msg(level: &str, tag: &str, msg: &str) {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let sec = (now / 1000) % 10000;
    let ms = now % 1000;
    println!("[{:04}.{:03}s] [{}] [{}] {}", sec, ms, level, tag, msg);
}

#[macro_export]
macro_rules! f_info {
    ($tag:expr, $($arg:tt)*) => {
        $crate::framework::log::log_msg("INFO", $tag, &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! f_warn {
    ($tag:expr, $($arg:tt)*) => {
        $crate::framework::log::log_msg("WARN", $tag, &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! f_error {
    ($tag:expr, $($arg:tt)*) => {
        $crate::framework::log::log_msg("ERROR", $tag, &format!($($arg)*))
    };
}
