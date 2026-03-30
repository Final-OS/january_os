use crate::fs;
use crate::task;
use crate::{error, kprintln, ok};

use alloc::format;
use alloc::string::String;

pub fn run() {
    match test_fd_ops_case() {
        Ok(()) => ok!("fs: fd_ops OK"),
        Err(msg) => error!("fs: fd_ops FAIL ({})", msg),
    }
}

fn test_fd_ops_case() -> Result<(), String> {
    let pid = task::current_pid()
        .map(|pid| pid.0)
        .ok_or_else(|| String::from("missing current pid"))?;

    let dirfd = fs::open_for_pid(pid, "/proc/self", 0, 0)
        .map_err(|errno| format!("open dir errno={errno}"))?;
    fs::fchdir_for_pid(pid, dirfd).map_err(|errno| format!("fchdir errno={errno}"))?;
    let cwd = fs::getcwd_for_pid(pid);
    if cwd != format!("/proc/{}", pid) {
        let _ = fs::close_for_pid(pid, dirfd);
        fs::drop_process_fds(pid);
        return Err(format!("unexpected cwd after fchdir: {}", cwd));
    }

    let filefd = fs::open_for_pid(pid, "/proc/self/status", 0, 0)
        .map_err(|errno| format!("open file errno={errno}"))?;
    let dupfd = fs::dup2_for_pid(pid, filefd, 31, true)
        .map_err(|errno| format!("dup3-like dup2 errno={errno}"))?;
    let cloexec = fs::fcntl_getfd_for_pid(pid, dupfd)
        .map_err(|errno| format!("fcntl getfd errno={errno}"))?;
    if cloexec != 1 {
        let _ = fs::close_for_pid(pid, dirfd);
        let _ = fs::close_for_pid(pid, filefd);
        let _ = fs::close_for_pid(pid, dupfd);
        fs::drop_process_fds(pid);
        return Err(format!("expected cloexec=1 got {}", cloexec));
    }

    let dupfd2 =
        fs::dup_for_pid(pid, filefd, 40, false).map_err(|errno| format!("dup errno={errno}"))?;
    let cloexec2 = fs::fcntl_getfd_for_pid(pid, dupfd2)
        .map_err(|errno| format!("fcntl getfd #2 errno={errno}"))?;
    if cloexec2 != 0 {
        let _ = fs::close_for_pid(pid, dirfd);
        let _ = fs::close_for_pid(pid, filefd);
        let _ = fs::close_for_pid(pid, dupfd);
        let _ = fs::close_for_pid(pid, dupfd2);
        fs::drop_process_fds(pid);
        return Err(format!("expected cloexec=0 got {}", cloexec2));
    }

    let _ = fs::close_for_pid(pid, dirfd);
    let _ = fs::close_for_pid(pid, filefd);
    let _ = fs::close_for_pid(pid, dupfd);
    let _ = fs::close_for_pid(pid, dupfd2);
    fs::drop_process_fds(pid);

    kprintln!("[test/fs] fd_ops verified fchdir + cloexec dup semantics");
    Ok(())
}
