//! UEFI 控制台输出辅助

use core::fmt::Write;

pub fn println_uefi(s: &str) {
    print_uefi(s);
    print_uefi("\r\n");
}

pub fn print_uefi(s: &str) {
    uefi::system::with_stdout(|stdout| {
        let _ = stdout.write_str(s);
    });
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

pub fn print_hex(mut val: u64) {
    let mut buf = [0u8; 16];
    let mut i = 16usize;

    print_uefi("0x");

    if val == 0 {
        print_uefi("0");
        return;
    }

    while val != 0 && i > 0 {
        i -= 1;
        let digit = (val & 0xF) as u8;
        buf[i] = if digit < 10 {
            b'0' + digit
        } else {
            b'a' + (digit - 10)
        };
        val >>= 4;
    }

    if let Ok(s) = core::str::from_utf8(&buf[i..16]) {
        print_uefi(s);
    }
}

pub fn print_bool(v: bool) {
    if v {
        print_uefi("true");
    } else {
        print_uefi("false");
    }
}
