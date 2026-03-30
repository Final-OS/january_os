use crate::fs;
use crate::task;
use crate::{error, kprintln, ok};

use alloc::format;
use alloc::string::String;

pub fn run() {
    match test_procfs_case() {
        Ok(()) => ok!("fs: procfs OK"),
        Err(msg) => error!("fs: procfs FAIL ({})", msg),
    }
}

fn test_procfs_case() -> Result<(), String> {
    let pid = task::current_pid()
        .map(|pid| pid.0)
        .ok_or_else(|| String::from("missing current pid"))?;

    let proc_stat =
        fs::stat_path_for_pid(pid, "/proc/self").map_err(|errno| format!("stat errno={errno}"))?;
    if (proc_stat.mode & 0o170000) != 0o040000 {
        return Err(format!(
            "/proc/self is not a directory: mode={:#o}",
            proc_stat.mode
        ));
    }

    let status = fs::read_all_for_pid(pid, "/proc/self/status")
        .map_err(|errno| format!("read status errno={errno}"))?;
    let status =
        core::str::from_utf8(&status).map_err(|_| String::from("status utf8 decode failed"))?;
    if !status.contains(format!("Pid:\t{}", pid).as_str()) {
        return Err(String::from("status missing pid field"));
    }

    let cpuinfo = fs::read_all_for_pid(pid, "/proc/cpuinfo")
        .map_err(|errno| format!("read cpuinfo errno={errno}"))?;
    let cpuinfo =
        core::str::from_utf8(&cpuinfo).map_err(|_| String::from("cpuinfo utf8 decode failed"))?;
    if !cpuinfo.contains("processor") {
        return Err(String::from("cpuinfo missing processor field"));
    }

    let meminfo = fs::read_all_for_pid(pid, "/proc/meminfo")
        .map_err(|errno| format!("read meminfo errno={errno}"))?;
    let meminfo =
        core::str::from_utf8(&meminfo).map_err(|_| String::from("meminfo utf8 decode failed"))?;
    if !meminfo.contains("MemTotal:") {
        return Err(String::from("meminfo missing MemTotal field"));
    }

    let cmdline = fs::read_all_for_pid(pid, "/proc/self/cmdline")
        .map_err(|errno| format!("read cmdline errno={errno}"))?;
    if cmdline.is_empty() {
        return Err(String::from("cmdline is empty"));
    }

    kprintln!(
        "[test/fs] procfs pid={} status/cpuinfo/meminfo/cmdline verified",
        pid
    );
    Ok(())
}
