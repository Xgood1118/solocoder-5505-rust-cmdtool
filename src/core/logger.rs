pub fn should_colorize() -> bool {
    if std::env::var("NO_COLOR").is_ok() {
        return false;
    }
    if std::env::var("TERM").map(|t| t == "dumb").unwrap_or(false) {
        return false;
    }
    atty::is(atty::Stream::Stderr)
}

macro_rules! log_info {
    ($($arg:tt)*) => {{
        eprintln!("[info] {}", format!($($arg)*));
    }};
}

macro_rules! log_warn {
    ($($arg:tt)*) => {{
        eprintln!("[warn] {}", format!($($arg)*));
    }};
}

macro_rules! log_error {
    ($($arg:tt)*) => {{
        eprintln!("[error] {}", format!($($arg)*));
    }};
}

pub(crate) use log_error;
pub(crate) use log_info;
pub(crate) use log_warn;

pub fn init() {
    if should_colorize() {
        colored::control::set_override(true);
    } else {
        colored::control::set_override(false);
    }
}
