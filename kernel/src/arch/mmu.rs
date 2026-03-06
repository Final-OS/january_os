use crate::component::{ComponentDescriptor, ComponentStage, ComponentStats};

pub trait ArchMmu {
    fn paging_root(&self) -> u64;
    fn flush_tlb(&self, vaddr: usize);
}

pub const COMPONENT: ComponentDescriptor = ComponentDescriptor {
    id: "arch_mmu",
    stage: ComponentStage::Early,
    deps: &["arch_cpu"],
    summary: "paging root and tlb control",
};

pub fn stats() -> ComponentStats {
    ComponentStats::ready()
}
