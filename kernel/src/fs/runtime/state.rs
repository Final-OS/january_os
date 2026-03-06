#[derive(Debug, Clone, Copy)]
pub struct FsRuntimeState {
    pub initramfs_present: bool,
    pub rootfs: &'static str,
}

impl FsRuntimeState {
    pub const fn placeholder() -> Self {
        Self {
            initramfs_present: false,
            rootfs: "initramfs",
        }
    }
}
