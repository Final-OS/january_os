#[derive(Debug, Clone, Copy)]
pub struct MmRuntimeRegistry {
    pub alloc_ready: bool,
    pub virt_ready: bool,
    pub dma_ready: bool,
}

impl MmRuntimeRegistry {
    pub const fn placeholder() -> Self {
        Self {
            alloc_ready: false,
            virt_ready: false,
            dma_ready: false,
        }
    }
}
