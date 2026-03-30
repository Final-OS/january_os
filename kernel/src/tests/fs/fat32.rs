use super::image;
use crate::fs;
use crate::{error, ok};

use alloc::format;
use core::sync::atomic::{AtomicUsize, Ordering};

static FAT_MOUNT_SEQ: AtomicUsize = AtomicUsize::new(1);

pub fn run() {
    match run_case() {
        Ok(()) => ok!("fs/fat32"),
        Err(msg) => error!("fs/fat32: {}", msg),
    }
}

fn run_case() -> Result<(), alloc::string::String> {
    let dev = image::build_fat32_device().map_err(|e| format!("build device failed: {}", e))?;
    let fs_obj = crate::fs::backing::Fat32FileSystem::mount(dev)
        .map_err(|e| format!("mount failed: {:?}", e))?;

    let seq = FAT_MOUNT_SEQ.fetch_add(1, Ordering::Relaxed);
    let mount = format!("/mnt_fat32_{}", seq);
    crate::fs::vfs::mount_fs(mount.as_str(), fs_obj).map_err(|e| format!("mount_fs: {:?}", e))?;

    let pid = 0x1000usize + seq;
    let fd = fs::runtime::open_for_pid(pid, format!("{}/HELLO.TXT", mount).as_str(), 0, 0)
        .map_err(|errno| format!("open HELLO.TXT errno={}", errno))?;
    let mut buf = [0u8; 32];
    let n = fs::runtime::read_for_pid(pid, fd, &mut buf)
        .map_err(|errno| format!("read HELLO.TXT errno={}", errno))?;
    if &buf[..n] != b"hello from fat32\n" {
        return Err(format!("HELLO.TXT mismatch: {:?}", &buf[..n]));
    }
    fs::runtime::close_for_pid(pid, fd).map_err(|errno| format!("close errno={}", errno))?;

    fs::runtime::chdir_for_pid(pid, format!("{}/SUBDIR", mount).as_str())
        .map_err(|errno| format!("chdir errno={}", errno))?;
    let cwd = fs::runtime::getcwd_for_pid(pid);
    if cwd != format!("{}/SUBDIR", mount) {
        return Err(format!("cwd mismatch: {}", cwd));
    }
    let sub_fd = fs::runtime::open_for_pid(pid, "NEST.TXT", 0, 0)
        .map_err(|errno| format!("open NEST.TXT errno={}", errno))?;
    let mut sub = [0u8; 32];
    let sub_n = fs::runtime::read_for_pid(pid, sub_fd, &mut sub)
        .map_err(|errno| format!("read NEST.TXT errno={}", errno))?;
    if &sub[..sub_n] != b"nested fat32\n" {
        return Err(format!("NEST.TXT mismatch: {:?}", &sub[..sub_n]));
    }
    fs::runtime::close_for_pid(pid, sub_fd)
        .map_err(|errno| format!("close nested errno={}", errno))?;

    let elf = fs::runtime::read_all_for_pid(pid, format!("{}/APP.ELF", mount).as_str())
        .map_err(|errno| format!("read APP.ELF errno={}", errno))?;
    crate::task::build_elf_load_plan(elf.as_slice())
        .map_err(|errno| format!("build_elf_load_plan errno={}", errno))?;

    let long_fd = fs::runtime::open_for_pid(pid, format!("{}/LONG-FILE.TXT", mount).as_str(), 0, 0)
        .map_err(|errno| format!("open LONG-FILE.TXT errno={}", errno))?;
    let mut long_buf = [0u8; 32];
    let long_n = fs::runtime::read_for_pid(pid, long_fd, &mut long_buf)
        .map_err(|errno| format!("read LONG-FILE.TXT errno={}", errno))?;
    if &long_buf[..long_n] != b"long fat32 name\n" {
        return Err(format!("LONG-FILE.TXT mismatch: {:?}", &long_buf[..long_n]));
    }
    fs::runtime::close_for_pid(pid, long_fd)
        .map_err(|errno| format!("close long name errno={}", errno))?;

    fs::runtime::drop_process_fds(pid);
    crate::fs::vfs::umount_fs(mount.as_str()).map_err(|e| format!("umount_fs: {:?}", e))?;
    Ok(())
}
