#![no_std]
#![no_main]

use january_user_coreutils as coreutils;
use january_user_runtime as rt;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    rt::write_line("pwd: panic");
    rt::exit(1);
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let code = coreutils::cmd_pwd();
    rt::exit(code);
}
