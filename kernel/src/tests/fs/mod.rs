//! 文件系统子系统测试。

mod ext4;
mod fat32;
mod image;
pub mod vfs;

pub fn run() {
    run_with_filter(None);
}

pub fn run_with_filter(filter: Option<&str>) {
    match filter {
        None | Some("all") => {
            vfs::run_with_filter(None);
            fat32::run();
            ext4::run();
        }
        Some("vfs") | Some("path") | Some("mount") | Some("fd_bridge") => {
            vfs::run_with_filter(filter);
        }
        Some("fat32") => fat32::run(),
        Some("ext4") => ext4::run(),
        Some(name) => {
            crate::error!("Unknown fs test: {}", name);
            crate::kprintln!("Available fs tests: vfs, fat32, ext4");
        }
    }
}
