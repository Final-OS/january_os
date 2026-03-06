#[derive(Debug, Clone, Copy)]
pub struct FsRuntimeRegistry {
    pub fd_runtime_ready: bool,
    pub vfs_ready: bool,
    pub pipe_ready: bool,
}

impl FsRuntimeRegistry {
    pub const fn placeholder() -> Self {
        Self {
            fd_runtime_ready: false,
            vfs_ready: false,
            pipe_ready: false,
        }
    }
}
