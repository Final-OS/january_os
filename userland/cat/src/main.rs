#![no_std]
#![no_main]

use january_user_coreutils as coreutils;
use january_user_runtime as rt;

const MAX_ARGS: usize = 8;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    rt::write_line("cat: panic");
    rt::exit(1);
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let (argc, argv) = unsafe { rt::argc_argv() };
    let mut args = [""; MAX_ARGS];
    let mut count = 0usize;
    let upper = argc.min(MAX_ARGS + 1);
    for idx in 1..upper {
        if let Some(text) = unsafe { rt::argv_at(argv, idx) } {
            args[count] = text;
            count += 1;
        }
    }
    let code = coreutils::cmd_cat(&args[..count]);
    rt::exit(code);
}
