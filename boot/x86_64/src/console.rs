//! UEFI 控制台输出辅助

use core::fmt::Write;

/// Debug mode - set to true to enable detailed output
pub const DEBUG: bool = false;

pub fn println_uefi(s: &str) {
    print_uefi(s);
    print_uefi("\r\n");
}

pub fn print_uefi(s: &str) {
    uefi::system::with_stdout(|stdout| {
        let _ = stdout.write_str(s);
    });
}

/// Diagnostic output - only shown in DEBUG mode
pub fn print_diag(s: &str) {
    if DEBUG {
        print_uefi("      ");
        println_uefi(s);
    }
}

pub fn print_stage(step: u8, title: &str) {
    print_uefi("[");
    print_dec(step as u64);
    print_uefi("/8] ");
    println_uefi(title);
}

pub fn print_dec(val: u64) {
    let mut buf = [b'0'; 20];
    let mut v = val;
    let mut i = 20;

    if v == 0 {
        print_uefi("0");
        return;
    }

    while v > 0 && i > 0 {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }

    if let Ok(s) = core::str::from_utf8(&buf[i..20]) {
        print_uefi(s);
    }
}
