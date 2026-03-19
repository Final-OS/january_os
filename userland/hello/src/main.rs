#![no_std]
#![no_main]

use january_user_runtime as rt;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    rt::write_line("hello: panic");
    rt::exit(1);
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    rt::write_line("HELLO");
    rt::exit(0);
}
