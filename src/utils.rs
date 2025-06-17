#[macro_export]
macro_rules! info {
    ($($arg:tt)+) => {{
        let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let level = "\x1b[34mINFO\x1b[0m";
        let body = format!($($arg)+);
        let msg = format!("{} [{}] {}", ts, level, body);
        let _ = $crate::PB.println(&msg);
    }};
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)+) => {{
        let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let level = "\x1b[31mERROR\x1b[0m";
        let body = format!($($arg)+);
        let msg = format!("{} [{}] {}", ts, level, body);
        let _ = $crate::PB.println(&msg);
    }};
}
