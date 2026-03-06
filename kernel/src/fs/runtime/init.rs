pub fn init(initramfs: Option<(u64, u64)>) -> crate::fs::FsInitReport {
    crate::fs::runtime::manager::init_runtime(initramfs)
}
