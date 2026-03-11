pub mod dma_coherent_guard;
pub mod swiotlb;

pub(super) use super::{fail, mm_step, pass};

pub(super) fn run_dma_coherent_guard() {
    dma_coherent_guard::run();
}
pub(super) fn run_swiotlb() {
    swiotlb::run();
}
