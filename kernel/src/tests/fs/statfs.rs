use crate::fs;
use crate::task;
use crate::{error, kprintln, ok};

use alloc::format;
use alloc::string::String;

pub fn run() {
    match test_statfs_case() {
        Ok(()) => ok!("fs: statfs OK"),
        Err(msg) => error!("fs: statfs FAIL ({})", msg),
    }
}

fn test_statfs_case() -> Result<(), String> {
    let pid = task::current_pid()
        .map(|pid| pid.0)
        .ok_or_else(|| String::from("missing current pid"))?;

    let root = fs::statfs_path("/").map_err(|errno| format!("statfs / errno={errno}"))?;
    if root.f_bsize <= 0 {
        return Err(String::from("root statfs invalid block size"));
    }

    let proc = fs::statfs_path_for_pid(pid, "/proc/self")
        .map_err(|errno| format!("statfs proc errno={errno}"))?;
    if proc.f_namelen < 255 {
        return Err(format!("procfs namelen too small: {}", proc.f_namelen));
    }

    let fd = fs::open_for_pid(pid, "/proc/self/status", 0, 0)
        .map_err(|errno| format!("open errno={errno}"))?;
    let fd_stat = fs::statfs_fd(pid, fd).map_err(|errno| format!("fstatfs errno={errno}"))?;
    let _ = fs::close_for_pid(pid, fd);
    fs::drop_process_fds(pid);

    if fd_stat.f_type != proc.f_type {
        return Err(format!(
            "fstatfs type mismatch: fd_type={:#x} path_type={:#x}",
            fd_stat.f_type, proc.f_type
        ));
    }

    kprintln!(
        "[test/fs] statfs root_bsize={} proc_type={:#x}",
        root.f_bsize,
        proc.f_type
    );
    Ok(())
}
