use crate::mm::boot::setup::MmInitStage;

#[derive(Debug, Clone, Copy)]
pub struct MmRuntimeState {
    pub stage: MmInitStage,
    pub alloc_ready: bool,
    pub virt_ready: bool,
    pub dma_ready: bool,
}

impl MmRuntimeState {
    pub fn snapshot() -> Self {
        let stage = crate::mm::boot::setup::init_stage();
        Self {
            stage,
            alloc_ready: matches!(stage, MmInitStage::Slub | MmInitStage::Complete),
            virt_ready: matches!(stage, MmInitStage::Complete),
            dma_ready: matches!(stage, MmInitStage::Complete),
        }
    }
}
