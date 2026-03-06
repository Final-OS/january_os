//! 兼容旧的 `test vfs` 入口。

pub fn run() {
    crate::tests::fs::run();
}

pub fn run_with_filter(filter: Option<&str>) {
    crate::tests::fs::run_with_filter(filter);
}
