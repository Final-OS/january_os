#![no_std]
#![no_main]

use january_user_coreutils as coreutils;
use january_user_runtime as rt;

const MAX_LINE: usize = 256;
const MAX_TOKENS: usize = 16;
const MAX_PATH: usize = 256;
const MAX_ARG_BYTES: usize = 96;
const DEFAULT_PATH: &str = "/bin:/usr/bin";
const PATH_DIRS: [&str; 2] = ["/bin", "/usr/bin"];

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    rt::write_line("sh: panic");
    rt::exit(1);
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let code = main();
    rt::exit(code);
}

fn read_line(buf: &mut [u8]) -> Option<usize> {
    let mut len = 0usize;
    loop {
        let mut ch = [0u8; 1];
        let ret = rt::read(0, ch.as_mut_ptr(), 1);
        if ret < 0 {
            let errno = rt::errno_from_ret(ret);
            if errno == 11 {
                continue;
            }
            return None;
        }
        if ret == 0 {
            return None;
        }

        let c = ch[0];
        match c {
            b'\n' => {
                rt::write_str("\n");
                return Some(len);
            }
            8 | 127 => {
                if len > 0 {
                    len -= 1;
                    rt::write_str("\x08 \x08");
                }
            }
            _ if c.is_ascii_graphic() || c == b' ' => {
                if len + 1 < buf.len() {
                    buf[len] = c;
                    len += 1;
                    let _ = rt::write(1, &c as *const u8, 1);
                }
            }
            _ => {}
        }
    }
}

fn parse_tokens<'a>(line: &'a str, out: &mut [&'a str; MAX_TOKENS]) -> usize {
    let mut count = 0usize;
    for token in line.split_whitespace() {
        if count >= out.len() {
            break;
        }
        out[count] = token;
        count += 1;
    }
    count
}

fn exec_external(tokens: &[&str]) -> i32 {
    if tokens.is_empty() {
        return 0;
    }

    let mut arg_bytes = [[0u8; MAX_ARG_BYTES]; MAX_TOKENS];
    let mut argv_ptrs: [*const u8; MAX_TOKENS + 1] = [core::ptr::null(); MAX_TOKENS + 1];
    let argc = tokens.len().min(MAX_TOKENS);
    for idx in 0..argc {
        let bytes = tokens[idx].as_bytes();
        if bytes.len().saturating_add(1) > arg_bytes[idx].len() {
            rt::write_line("sh: argument too long");
            return 1;
        }
        arg_bytes[idx][..bytes.len()].copy_from_slice(bytes);
        arg_bytes[idx][bytes.len()] = 0;
        argv_ptrs[idx] = arg_bytes[idx].as_ptr();
    }
    argv_ptrs[argc] = core::ptr::null();
    let path_env = b"PATH=/bin:/usr/bin\0";
    let envp: [*const u8; 2] = [path_env.as_ptr(), core::ptr::null()];
    let mut path_cstr = [0u8; MAX_PATH];

    if tokens[0].as_bytes().contains(&b'/') {
        if tokens[0].len().saturating_add(1) > path_cstr.len() {
            rt::write_line("sh: command path too long");
            return 1;
        }
        path_cstr[..tokens[0].len()].copy_from_slice(tokens[0].as_bytes());
        path_cstr[tokens[0].len()] = 0;
        let ret = rt::execve(path_cstr.as_ptr(), argv_ptrs.as_ptr(), envp.as_ptr());
        if ret >= 0 {
            return 0;
        }

        rt::write_str("sh: command not found: ");
        rt::write_line(tokens[0]);
        return 127;
    }

    for dir in PATH_DIRS.iter() {
        let need = dir
            .len()
            .saturating_add(1)
            .saturating_add(tokens[0].len())
            .saturating_add(1);
        if need > path_cstr.len() {
            continue;
        }

        let mut offset = 0usize;
        path_cstr[..dir.len()].copy_from_slice(dir.as_bytes());
        offset += dir.len();
        path_cstr[offset] = b'/';
        offset += 1;
        path_cstr[offset..offset + tokens[0].len()].copy_from_slice(tokens[0].as_bytes());
        offset += tokens[0].len();
        path_cstr[offset] = 0;

        let ret = rt::execve(path_cstr.as_ptr(), argv_ptrs.as_ptr(), envp.as_ptr());
        if ret >= 0 {
            return 0;
        }
        if rt::errno_from_ret(ret) != 2 {
            rt::write_str("sh: exec error errno=");
            rt::write_u64(rt::errno_from_ret(ret) as u64);
            rt::write_str("\n");
            return 126;
        }
    }

    rt::write_str("sh: command not found: ");
    rt::write_line(tokens[0]);
    127
}

fn main() -> i32 {
    rt::write_line("january sh");
    rt::write_line("commands: help, ls, cat, pwd, cd, echo, exit");
    rt::write_str("PATH=");
    rt::write_line(DEFAULT_PATH);

    let mut line = [0u8; MAX_LINE];
    loop {
        rt::write_str("> ");
        let Some(len) = read_line(&mut line) else {
            continue;
        };
        if len == 0 {
            continue;
        }

        let Ok(line_str) = core::str::from_utf8(&line[..len]) else {
            rt::write_line("sh: invalid utf8 input");
            continue;
        };

        let mut tokens = [""; MAX_TOKENS];
        let count = parse_tokens(line_str, &mut tokens);
        if count == 0 {
            continue;
        }

        let cmd = tokens[0];
        let args = &tokens[1..count];
        match cmd {
            "help" => {
                rt::write_line("help                show this help");
                rt::write_line("ls [path]           list directory");
                rt::write_line("cat <path> [...]    print file");
                rt::write_line("pwd                 show cwd");
                rt::write_line("cd <path>           change cwd");
                rt::write_line("echo <text...>      print text");
                rt::write_line("exit                exit shell");
            }
            "ls" => {
                let _ = coreutils::cmd_ls(args.first().copied());
            }
            "cat" => {
                let _ = coreutils::cmd_cat(args);
            }
            "pwd" => {
                let _ = coreutils::cmd_pwd();
            }
            "cd" => {
                if let Some(path) = args.first().copied() {
                    let _ = coreutils::cmd_cd(path);
                } else {
                    rt::write_line("usage: cd <path>");
                }
            }
            "echo" => {
                let _ = coreutils::cmd_echo(args);
            }
            "exit" => return 0,
            _ => {
                let _ = exec_external(&tokens[..count]);
            }
        }
    }
}
