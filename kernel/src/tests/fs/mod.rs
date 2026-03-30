//! 文件系统子系统测试。

mod ext4;
mod fat32;
mod fd_ops;
mod image;
mod open_access;
mod procfs;
mod statfs;
pub mod vfs;

pub fn run() {
    run_with_filter(None);
}

pub fn run_with_filter(filter: Option<&str>) {
    match filter {
        None | Some("all") => {
            vfs::run_with_filter(None);
            fd_ops::run();
            open_access::run();
            procfs::run();
            statfs::run();
            fat32::run();
            ext4::run();
        }
        Some("open_access") => open_access::run(),
        Some("fd_ops") => fd_ops::run(),
        Some("statfs") => statfs::run(),
        Some("procfs") => procfs::run(),
        Some("vfs") | Some("path") | Some("mount") | Some("fd_bridge") => {
            vfs::run_with_filter(filter);
        }
        Some("fat32") => fat32::run(),
        Some("ext4") => ext4::run(),
        Some(name) => {
            crate::error!("Unknown fs test: {}", name);
            crate::kprintln!(
                "Available fs tests: vfs, fd_ops, open_access, procfs, statfs, fat32, ext4"
            );
        }
    }
}
