use super::*;

pub(crate) use crate::common::uaccess::{
    read_user_cstring, read_user_struct, validate_user_range, write_user_struct,
};

#[inline]
pub(crate) fn current_pid_raw() -> Result<usize, i32> {
    task::current_pid().map(|pid| pid.0).ok_or(ESRCH)
}

#[inline]
pub(crate) fn linux_stat_from_fs(meta: fs::FsStat) -> LinuxStat {
    LinuxStat {
        st_dev: meta.dev,
        st_ino: meta.ino,
        st_nlink: meta.nlink,
        st_mode: meta.mode,
        st_uid: 0,
        st_gid: 0,
        __pad0: 0,
        st_rdev: meta.rdev,
        st_size: meta.size,
        st_blksize: meta.blksize,
        st_blocks: meta.blocks,
        st_atim: LinuxTimespec::default(),
        st_mtim: LinuxTimespec::default(),
        st_ctim: LinuxTimespec::default(),
        __glibc_reserved: [0; 3],
    }
}
