//! 文件系统子系统测试。

pub mod vfs;

pub fn run() {
    run_with_filter(None);
}

pub fn run_with_filter(filter: Option<&str>) {
    vfs::run_with_filter(filter);
}
