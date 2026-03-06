#![no_std]

use core::mem::size_of;

use january_user_runtime as rt;

const DT_DIR: u8 = 4;
const DT_LNK: u8 = 10;
const DT_REG: u8 = 8;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct LinuxDirent64Fixed {
    d_ino: u64,
    d_off: i64,
    d_reclen: u16,
    d_type: u8,
}

#[inline]
fn print_errno(prefix: &str, errno: i32) {
    rt::write_str(prefix);
    rt::write_str(": errno=");
    rt::write_u64(errno as u64);
    rt::write_str("\n");
}

pub fn cmd_pwd() -> i32 {
    let mut buf = [0u8; 512];
    let ret = rt::getcwd(buf.as_mut_ptr(), buf.len());
    if ret < 0 {
        print_errno("pwd", rt::errno_from_ret(ret));
        return 1;
    }
    let mut len = 0usize;
    while len < buf.len() && buf[len] != 0 {
        len += 1;
    }
    let _ = rt::write(1, buf.as_ptr(), len);
    rt::write_str("\n");
    0
}

pub fn cmd_cd(path: &str) -> i32 {
    let mut cstr = [0u8; 256];
    let Some(ret) = rt::with_cstr(path, &mut cstr, |ptr| rt::chdir(ptr)) else {
        rt::write_line("cd: path too long");
        return 1;
    };
    if ret < 0 {
        print_errno("cd", rt::errno_from_ret(ret));
        return 1;
    }
    0
}

pub fn cmd_echo(args: &[&str]) -> i32 {
    for (index, arg) in args.iter().enumerate() {
        if index > 0 {
            rt::write_str(" ");
        }
        rt::write_str(arg);
    }
    rt::write_str("\n");
    0
}

pub fn cmd_cat(args: &[&str]) -> i32 {
    if args.is_empty() {
        rt::write_line("usage: cat <path> [...]");
        return 1;
    }

    let mut status = 0;
    let mut path_cstr = [0u8; 256];
    let mut buf = [0u8; 512];
    for path in args {
        let Some(fd) = rt::with_cstr(path, &mut path_cstr, |ptr| rt::open(ptr, rt::O_RDONLY, 0))
        else {
            rt::write_line("cat: path too long");
            status = 1;
            continue;
        };
        if fd < 0 {
            print_errno("cat", rt::errno_from_ret(fd));
            status = 1;
            continue;
        }
        loop {
            let n = rt::read(fd as i32, buf.as_mut_ptr(), buf.len());
            if n < 0 {
                print_errno("cat", rt::errno_from_ret(n));
                status = 1;
                break;
            }
            if n == 0 {
                break;
            }
            let _ = rt::write(1, buf.as_ptr(), n as usize);
        }
        let _ = rt::close(fd as i32);
    }
    status
}

pub fn cmd_ls(path: Option<&str>) -> i32 {
    let target = path.unwrap_or(".");
    let mut path_cstr = [0u8; 256];
    let Some(fd_ret) = rt::with_cstr(target, &mut path_cstr, |ptr| rt::open(ptr, rt::O_RDONLY, 0))
    else {
        rt::write_line("ls: path too long");
        return 1;
    };

    if fd_ret < 0 {
        print_errno("ls", rt::errno_from_ret(fd_ret));
        return 1;
    }

    let fd = fd_ret as i32;
    let mut buf = [0u8; 1024];
    loop {
        let n = rt::getdents64(fd, buf.as_mut_ptr(), buf.len());
        if n < 0 {
            print_errno("ls", rt::errno_from_ret(n));
            let _ = rt::close(fd);
            return 1;
        }
        if n == 0 {
            break;
        }

        let mut pos = 0usize;
        let limit = n as usize;
        while pos < limit {
            if pos.saturating_add(size_of::<LinuxDirent64Fixed>()) > limit {
                break;
            }

            let fixed =
                unsafe { core::ptr::read_unaligned(buf.as_ptr().add(pos) as *const LinuxDirent64Fixed) };
            if fixed.d_reclen == 0 {
                break;
            }

            let reclen = fixed.d_reclen as usize;
            if pos.saturating_add(reclen) > limit {
                break;
            }

            let name_ptr = unsafe { buf.as_ptr().add(pos + size_of::<LinuxDirent64Fixed>()) };
            let max_name = reclen.saturating_sub(size_of::<LinuxDirent64Fixed>());
            let mut name_len = 0usize;
            while name_len < max_name {
                let b = unsafe { *name_ptr.add(name_len) };
                if b == 0 {
                    break;
                }
                name_len += 1;
            }

            let tag = match fixed.d_type {
                DT_DIR => "<dir>",
                DT_REG => "<file>",
                DT_LNK => "<link>",
                _ => "<other>",
            };
            rt::write_str(tag);
            rt::write_str(" ");

            if name_len > 0 {
                let _ = rt::write(1, name_ptr, name_len);
            }
            rt::write_str("\n");
            pos = pos.saturating_add(reclen);
        }
    }

    let _ = rt::close(fd);
    0
}
