#![no_std]
#![no_main]

use january_user_coreutils as coreutils;
use january_user_runtime as rt;

const MAX_LINE: usize = 256;
const MAX_TOKENS: usize = 16;
const ROOT_PATH: &[u8] = b"/\0";

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    rt::write_line("init: panic");
    rt::exit(111);
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

fn main() -> i32 {
    let _ = rt::chdir(ROOT_PATH.as_ptr());

    rt::write_line("january init");
    rt::write_line("PID 1 user-space shell");
    rt::write_line("commands: help, ls, cat, pwd, cd, echo, exit");

    let mut line = [0u8; MAX_LINE];
    loop {
        rt::write_str("init# ");
        let Some(len) = read_line(&mut line) else {
            continue;
        };
        if len == 0 {
            continue;
        }

        let Ok(line_str) = core::str::from_utf8(&line[..len]) else {
            rt::write_line("init: invalid utf8 input");
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
                rt::write_line("exit                exit PID 1 shell");
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
                rt::write_str("init: unknown command: ");
                rt::write_line(cmd);
            }
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    rt::exit(main());
}
