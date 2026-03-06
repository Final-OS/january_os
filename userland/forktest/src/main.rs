#![no_std]
#![no_main]

use january_user_runtime as rt;

const CHILD_VALUE: u64 = 0xAABB_CCDD_EEFF_0011;
const PARENT_VALUE: u64 = 0x5566_7788_99AA_BBCC;
const INITIAL_VALUE: u64 = 0x1122_3344_5566_7788;
const CHILD_EXIT: i32 = 7;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    rt::write_line("forktest: panic");
    rt::exit(111);
}

fn main() -> i32 {
    let page = rt::mmap(
        0,
        4096,
        rt::PROT_READ | rt::PROT_WRITE,
        rt::MAP_PRIVATE | rt::MAP_ANONYMOUS,
        usize::MAX,
        0,
    );
    if page < 0 {
        return 100;
    }

    let page_ptr = page as usize as *mut u64;
    unsafe {
        *page_ptr = INITIAL_VALUE;
    }

    let pid = rt::fork();
    if pid < 0 {
        return 101;
    }

    if pid == 0 {
        unsafe {
            *page_ptr = CHILD_VALUE;
            if *page_ptr != CHILD_VALUE {
                rt::exit(102);
            }
        }
        rt::exit(CHILD_EXIT);
    }

    let mut status = 0i32;
    let waited = rt::wait4(pid as i32, &mut status as *mut i32, 0, core::ptr::null_mut());
    if waited != pid {
        return 103;
    }
    if status != (CHILD_EXIT << 8) {
        return 104;
    }

    unsafe {
        if *page_ptr != INITIAL_VALUE {
            return 105;
        }
        *page_ptr = PARENT_VALUE;
        if *page_ptr != PARENT_VALUE {
            return 106;
        }
    }

    0
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    rt::exit(main());
}
