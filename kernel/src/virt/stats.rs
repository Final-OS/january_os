#[derive(Debug, Clone, Copy)]
pub struct VirtStats {
    pub init_attempts: u32,
    pub vm_creates: u32,
    pub vcpu_runs: u32,
    pub irq_injections: u32,
    pub mmio_regions: u32,
    pub memslot_updates: u32,
}

impl VirtStats {
    pub const fn placeholder() -> Self {
        Self {
            init_attempts: 1,
            vm_creates: 0,
            vcpu_runs: 0,
            irq_injections: 0,
            mmio_regions: 0,
            memslot_updates: 0,
        }
    }
}
