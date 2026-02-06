//! 内核日志宏
//!
//! 提供统一的日志输出格式

use core::fmt;

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {{
        // Bright Cyan [ INFO ]
        $crate::kprintln!("\x1b[96m[ INFO ]\x1b[0m {}", format_args!($($arg)*));
    }};
}

#[macro_export]
macro_rules! ok {
    ($($arg:tt)*) => {{
        // Bright Green [  OK  ]
        $crate::kprintln!("\x1b[92m[  OK  ]\x1b[0m {}", format_args!($($arg)*));
    }};
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {{
        // Bright Yellow [ WARN ]
        $crate::kprintln!("\x1b[93m[ WARN ]\x1b[0m {}", format_args!($($arg)*));
    }};
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {{
        // Bright Red [ ERRO ]
        $crate::kprintln!("\x1b[91m[ ERRO ]\x1b[0m {}", format_args!($($arg)*));
    }};
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {{
        // Gray [ DEBG ]
        $crate::kprintln!("\x1b[90m[ DEBG ]\x1b[0m {}", format_args!($($arg)*));
    }};
}
