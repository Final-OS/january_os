use crate::fs;
use crate::{kprint, kprintln};

const SHELL_FS_PID: usize = 0x5348_454c;
const O_RDONLY: u32 = 0;
const DT_DIR: u8 = 4;
const DT_REG: u8 = 8;
const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;

fn print_ls_entry(name: &str, file_type: u8) {
    let kind = match file_type {
        DT_DIR => "d",
        DT_REG => "f",
        _ => "?",
    };
    kprintln!("{} {}", kind, name);
}

fn print_cat_bytes(buf: &[u8]) {
    if let Ok(text) = core::str::from_utf8(buf) {
        kprint!("{}", text);
        return;
    }

    for &b in buf {
        if b.is_ascii_graphic() || b == b' ' || b == b'\n' || b == b'\r' || b == b'\t' {
            kprint!("{}", b as char);
        } else {
            kprint!(".");
        }
    }
}

pub(super) fn execute_ls_command(args: &[&str]) {
    let path = args.first().copied().unwrap_or(".");
    let stat = match fs::stat_path_for_pid(SHELL_FS_PID, path) {
        Ok(stat) => stat,
        Err(errno) => {
            kprintln!("ls: {} (errno={})", path, errno);
            return;
        }
    };

    if (stat.mode & S_IFMT) != S_IFDIR {
        print_ls_entry(path, DT_REG);
        return;
    }

    let fd = match fs::open_for_pid(SHELL_FS_PID, path, O_RDONLY, 0) {
        Ok(fd) => fd,
        Err(errno) => {
            kprintln!("ls: failed to open {} (errno={})", path, errno);
            return;
        }
    };

    loop {
        let entry = match fs::peek_dir_entry_for_pid(SHELL_FS_PID, fd) {
            Ok(entry) => entry,
            Err(errno) => {
                kprintln!("ls: failed to read {} (errno={})", path, errno);
                break;
            }
        };

        let Some(entry) = entry else {
            break;
        };
        print_ls_entry(entry.name.as_str(), entry.file_type);
        if let Err(errno) = fs::advance_dir_cursor_for_pid(SHELL_FS_PID, fd, 1) {
            kprintln!("ls: failed to advance {} (errno={})", path, errno);
            break;
        }
    }

    let _ = fs::close_for_pid(SHELL_FS_PID, fd);
}

pub(super) fn execute_cd_command(args: &[&str]) {
    let Some(path) = args.first().copied() else {
        kprintln!("usage: cd <path>");
        return;
    };
    if let Err(errno) = fs::chdir_for_pid(SHELL_FS_PID, path) {
        kprintln!("cd: {} (errno={})", path, errno);
    }
}

pub(super) fn execute_pwd_command() {
    let cwd = fs::getcwd_for_pid(SHELL_FS_PID);
    kprintln!("{}", cwd);
}

pub(super) fn execute_cat_command(args: &[&str]) {
    if args.is_empty() {
        kprintln!("usage: cat <path> [...]");
        return;
    }

    for path in args.iter() {
        let fd = match fs::open_for_pid(SHELL_FS_PID, path, O_RDONLY, 0) {
            Ok(fd) => fd,
            Err(errno) => {
                kprintln!("cat: {} (errno={})", path, errno);
                continue;
            }
        };

        let mut buf = [0u8; 256];
        loop {
            let n = match fs::read_for_pid(SHELL_FS_PID, fd, &mut buf) {
                Ok(n) => n,
                Err(errno) => {
                    kprintln!("cat: {} read error (errno={})", path, errno);
                    break;
                }
            };
            if n == 0 {
                break;
            }
            print_cat_bytes(&buf[..n]);
        }

        let _ = fs::close_for_pid(SHELL_FS_PID, fd);
        kprintln!();
    }
}
