#![no_std]
#![no_main]

use january_user_coreutils as coreutils;
use january_user_runtime as rt;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    rt::write_line("ls: panic");
    rt::exit(1);
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let (argc, argv) = unsafe { rt::argc_argv() };
    let path = if argc > 1 {
        unsafe { rt::argv_at(argv, 1) }
    } else {
        None
    };
    let code = coreutils::cmd_ls(path);
    rt::exit(code);
}
