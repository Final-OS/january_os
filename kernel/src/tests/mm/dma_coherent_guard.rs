use super::{fail, mm_step, pass};
use crate::mm;
use crate::mm::iommu::{dma_alloc_coherent, dma_coherent_guard_stats, dma_free_coherent, DmaAddr};

pub(super) fn run() {
    const SIZE: usize = mm::PAGE_SIZE as usize;

    mm_step("dma_coherent_guard: case=invalid_free_args_rejected");
    let base = dma_coherent_guard_stats();

    let (virt, dma) = match dma_alloc_coherent(SIZE, mm::GFP_KERNEL) {
        Some(v) => v,
        None => return fail("dma_coherent_guard", "dma_alloc_coherent failed"),
    };

    dma_free_coherent(virt, dma, SIZE / 2);
    let after_size_mismatch = dma_coherent_guard_stats();
    if after_size_mismatch.free_size_mismatch != base.free_size_mismatch + 1 {
        return fail(
            "dma_coherent_guard",
            "size mismatch free did not increment guard counter",
        );
    }

    dma_free_coherent(virt, DmaAddr::new(dma.as_u64() + mm::PAGE_SIZE), SIZE);
    let after_dma_mismatch = dma_coherent_guard_stats();
    if after_dma_mismatch.free_dma_mismatch != after_size_mismatch.free_dma_mismatch + 1 {
        return fail(
            "dma_coherent_guard",
            "dma mismatch free did not increment guard counter",
        );
    }

    dma_free_coherent(virt, dma, SIZE);
    dma_free_coherent(virt, dma, SIZE);
    let after_meta_miss = dma_coherent_guard_stats();
    if after_meta_miss.free_meta_miss != after_dma_mismatch.free_meta_miss + 1 {
        return fail(
            "dma_coherent_guard",
            "double free did not trigger metadata miss counter",
        );
    }

    pass("dma_coherent_guard");
}
