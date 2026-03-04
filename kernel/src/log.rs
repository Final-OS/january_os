//! 内核日志宏
//!
//! 提供统一的日志输出格式

#[macro_export]
macro_rules! info {
    ($fmt:literal $(, $($arg:tt)+)?) => {{
        if $fmt.trim_start().starts_with('[') {
            $crate::kprintln!("\x1b[96m[INFO]\x1b[0m{}", format_args!($fmt $(, $($arg)+)?));
        } else {
            $crate::kprintln!(
                "\x1b[96m[INFO]\x1b[0m{}",
                format_args!(concat!("[core] ", $fmt) $(, $($arg)+)?)
            );
        }
    }};
    ($($arg:tt)*) => {{
        $crate::kprintln!("\x1b[96m[INFO]\x1b[0m{}", format_args!($($arg)*));
    }};
}

#[macro_export]
macro_rules! ok {
    ($fmt:literal $(, $($arg:tt)+)?) => {{
        if $fmt.trim_start().starts_with('[') {
            $crate::kprintln!("\x1b[92m[OK]\x1b[0m{}", format_args!($fmt $(, $($arg)+)?));
        } else {
            $crate::kprintln!(
                "\x1b[92m[OK]\x1b[0m{}",
                format_args!(concat!("[core] ", $fmt) $(, $($arg)+)?)
            );
        }
    }};
    ($($arg:tt)*) => {{
        $crate::kprintln!("\x1b[92m[OK]\x1b[0m{}", format_args!($($arg)*));
    }};
}

#[macro_export]
macro_rules! warn {
    ($fmt:literal $(, $($arg:tt)+)?) => {{
        if $fmt.trim_start().starts_with('[') {
            $crate::kprintln!("\x1b[93m[WARN]\x1b[0m{}", format_args!($fmt $(, $($arg)+)?));
        } else {
            $crate::kprintln!(
                "\x1b[93m[WARN]\x1b[0m{}",
                format_args!(concat!("[core] ", $fmt) $(, $($arg)+)?)
            );
        }
    }};
    ($($arg:tt)*) => {{
        $crate::kprintln!("\x1b[93m[WARN]\x1b[0m{}", format_args!($($arg)*));
    }};
}

#[macro_export]
macro_rules! error {
    ($fmt:literal $(, $($arg:tt)+)?) => {{
        if $fmt.trim_start().starts_with('[') {
            $crate::kprintln!("\x1b[91m[ERROR]\x1b[0m{}", format_args!($fmt $(, $($arg)+)?));
        } else {
            $crate::kprintln!(
                "\x1b[91m[ERROR]\x1b[0m{}",
                format_args!(concat!("[core] ", $fmt) $(, $($arg)+)?)
            );
        }
    }};
    ($($arg:tt)*) => {{
        $crate::kprintln!("\x1b[91m[ERROR]\x1b[0m{}", format_args!($($arg)*));
    }};
}

/// Diagnostic output - only shown when DEBUG_VERBOSE is enabled
#[macro_export]
macro_rules! diag {
    ($fmt:literal $(, $($arg:tt)+)?) => {{
        if $crate::config::DEBUG_VERBOSE {
            if $fmt.trim_start().starts_with('[') {
                $crate::kprintln!("\x1b[90m[diag]\x1b[0m{}", format_args!($fmt $(, $($arg)+)?));
            } else {
                $crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m{}",
                    format_args!(concat!("[core] ", $fmt) $(, $($arg)+)?)
                );
            }
        }
    }};
    ($($arg:tt)*) => {{
        if $crate::config::DEBUG_VERBOSE {
            $crate::kprintln!("\x1b[90m[diag]\x1b[0m{}", format_args!($($arg)*));
        }
    }};
}

#[macro_export]
macro_rules! debug {
    ($fmt:literal $(, $($arg:tt)+)?) => {{
        if $crate::config::DEBUG_VERBOSE {
            if $fmt.trim_start().starts_with('[') {
                $crate::kprintln!("\x1b[90m[DEBUG]\x1b[0m{}", format_args!($fmt $(, $($arg)+)?));
            } else {
                $crate::kprintln!(
                    "\x1b[90m[DEBUG]\x1b[0m{}",
                    format_args!(concat!("[core] ", $fmt) $(, $($arg)+)?)
                );
            }
        }
    }};
    ($($arg:tt)*) => {{
        if $crate::config::DEBUG_VERBOSE {
            $crate::kprintln!("\x1b[90m[DEBUG]\x1b[0m{}", format_args!($($arg)*));
        }
    }};
}
