use super::image;
use crate::fs;
use crate::{error, ok};

use alloc::format;
use core::sync::atomic::{AtomicUsize, Ordering};

static EXT4_MOUNT_SEQ: AtomicUsize = AtomicUsize::new(1);

pub fn run() {
    match run_case() {
        Ok(()) => ok!("fs/ext4"),
        Err(msg) => error!("fs/ext4: {}", msg),
    }
}

fn run_case() -> Result<(), alloc::string::String> {
    let dev = image::build_ext4_device().map_err(|e| format!("build device failed: {}", e))?;
    let fs_obj = crate::fs::backing::Ext4FileSystem::mount(dev)
        .map_err(|e| format!("mount failed: {:?}", e))?;

    let seq = EXT4_MOUNT_SEQ.fetch_add(1, Ordering::Relaxed);
    let mount = format!("/mnt_ext4_{}", seq);
    crate::fs::vfs::mount_fs(mount.as_str(), fs_obj).map_err(|e| format!("mount_fs: {:?}", e))?;

    let pid = 0x2000usize + seq;
    let fd = fs::runtime::open_for_pid(pid, format!("{}/hello.txt", mount).as_str(), 0, 0)
        .map_err(|errno| format!("open hello.txt errno={}", errno))?;
    let mut buf = [0u8; 32];
    let n = fs::runtime::read_for_pid(pid, fd, &mut buf)
        .map_err(|errno| format!("read hello.txt errno={}", errno))?;
    if &buf[..n] != b"hello from ext4\n" {
        return Err(format!("hello.txt mismatch: {:?}", &buf[..n]));
    }
    fs::runtime::close_for_pid(pid, fd).map_err(|errno| format!("close errno={}", errno))?;

    fs::runtime::chdir_for_pid(pid, mount.as_str())
        .map_err(|errno| format!("chdir errno={}", errno))?;
    let cwd = fs::runtime::getcwd_for_pid(pid);
    if cwd != mount {
        return Err(format!("cwd mismatch: {}", cwd));
    }

    let elf = fs::runtime::read_all_for_pid(pid, format!("{}/app.elf", mount).as_str())
        .map_err(|errno| format!("read app.elf errno={}", errno))?;
    crate::task::build_elf_load_plan(elf.as_slice())
        .map_err(|errno| format!("build_elf_load_plan errno={}", errno))?;

    let deep = fs::runtime::read_all_for_pid(pid, format!("{}/deep.txt", mount).as_str())
        .map_err(|errno| format!("read deep.txt errno={}", errno))?;
    let expected = super::image::build_deep_ext4_payload();
    if deep != expected {
        return Err(format!("deep.txt mismatch: got={} want={}", deep.len(), expected.len()));
    }

    fs::runtime::drop_process_fds(pid);
    crate::fs::vfs::umount_fs(mount.as_str()).map_err(|e| format!("umount_fs: {:?}", e))?;
    Ok(())
}
