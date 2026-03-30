use crate::fs;
use crate::task;
use crate::{error, kprintln, ok};

use alloc::format;
use alloc::string::String;

const O_RDONLY: u32 = 0;
const O_DIRECTORY: u32 = 0o200000;
const O_CLOEXEC: u32 = 0o2000000;
const R_OK: u32 = 4;
const W_OK: u32 = 2;
const X_OK: u32 = 1;

pub fn run() {
    match test_open_access_case() {
        Ok(()) => ok!("fs: open_access OK"),
        Err(msg) => error!("fs: open_access FAIL ({})", msg),
    }
}

fn test_open_access_case() -> Result<(), String> {
    let pid = task::current_pid()
        .map(|pid| pid.0)
        .ok_or_else(|| String::from("missing current pid"))?;

    fs::access_path_for_pid(pid, "/proc/self/status", R_OK)
        .map_err(|errno| format!("access R_OK errno={errno}"))?;

    if fs::access_path_for_pid(pid, "/proc/self/status", W_OK).is_ok() {
        return Err(String::from("expected W_OK on readonly procfs to fail"));
    }
    if fs::access_path_for_pid(pid, "/proc/self/status", X_OK).is_ok() {
        return Err(String::from("expected X_OK on status file to fail"));
    }

    let dirfd = fs::open_for_pid(pid, "/proc/self", O_RDONLY | O_DIRECTORY | O_CLOEXEC, 0)
        .map_err(|errno| format!("open dir flags errno={errno}"))?;
    let cloexec = fs::fcntl_getfd_for_pid(pid, dirfd)
        .map_err(|errno| format!("fcntl getfd errno={errno}"))?;
    if cloexec != 1 {
        let _ = fs::close_for_pid(pid, dirfd);
        fs::drop_process_fds(pid);
        return Err(format!("expected cloexec=1 got {}", cloexec));
    }

    let _ = fs::close_for_pid(pid, dirfd);
    fs::drop_process_fds(pid);

    kprintln!("[test/fs] open_access verified readonly access + open flags");
    Ok(())
}
