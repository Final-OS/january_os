//! 内存管理子系统测试

mod alloc;
mod dma;
mod phys;
mod virt;

use crate::{error, kprintln, ok};
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

static MM_STEP_SEQ: AtomicUsize = AtomicUsize::new(0);
static MM_FAILED: AtomicBool = AtomicBool::new(false);

pub(super) fn mm_step(msg: &str) {
    let seq = MM_STEP_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    if crate::config::DEBUG_VERBOSE {
        kprintln!("[test/mm][step {}] {}", seq, msg);
    }
}

pub fn run() {
    run_with_filter(None);
}

pub fn run_with_filter(filter: Option<&str>) {
    MM_STEP_SEQ.store(0, Ordering::SeqCst);
    MM_FAILED.store(false, Ordering::SeqCst);
    kprintln!("=== MM Subsystem Tests ===");
    mm_step("start mm test suite");
    if crate::config::DEBUG_VERBOSE {
        kprintln!("[test/mm] filter={:?}", filter);
    }

    match filter {
        None | Some("all") => {
            run_case("run case=swiotlb", dma::run_swiotlb);
            run_case("run case=dma_coherent_guard", dma::run_dma_coherent_guard);
            run_case("run case=fork_cow", virt::run_fork_cow);
            run_case("run case=slub", alloc::run_slub);
            run_case("run case=buddy", phys::run_buddy);
            run_case("run case=page_counter_guard", phys::run_page_counter_guard);
            run_case("run case=status_readonly", phys::run_status_readonly);
            run_case("run case=pcp", phys::run_pcp);
            run_case("run case=heap", alloc::run_heap);
            run_case("run case=mmap", virt::run_mmap);
            run_case("run case=pt_ownership", virt::run_pt_ownership);
            run_case("run case=pt_reclaim", virt::run_pt_reclaim);
            run_case("run case=vmalloc_heal", alloc::run_vmalloc_heal);
        }
        Some("swiotlb") => {
            mm_step("run case=swiotlb");
            dma::run_swiotlb();
        }
        Some("slub") => {
            mm_step("run case=slub");
            alloc::run_slub();
        }
        Some("dma_coherent_guard") => {
            mm_step("run case=dma_coherent_guard");
            dma::run_dma_coherent_guard();
        }
        Some("fork_cow") => {
            mm_step("run case=fork_cow");
            virt::run_fork_cow();
        }
        Some("buddy") => {
            mm_step("run case=buddy");
            phys::run_buddy();
        }
        Some("page_counter_guard") => {
            mm_step("run case=page_counter_guard");
            phys::run_page_counter_guard();
        }
        Some("status_readonly") => {
            mm_step("run case=status_readonly");
            phys::run_status_readonly();
        }
        Some("pcp") => {
            mm_step("run case=pcp");
            phys::run_pcp();
        }
        Some("heap") => {
            mm_step("run case=heap");
            alloc::run_heap();
        }
        Some("mmap") => {
            mm_step("run case=mmap");
            virt::run_mmap();
        }
        Some("pt_ownership") => {
            mm_step("run case=pt_ownership");
            virt::run_pt_ownership();
        }
        Some("pt_reclaim") => {
            mm_step("run case=pt_reclaim");
            virt::run_pt_reclaim();
        }
        Some("vmalloc_heal") => {
            mm_step("run case=vmalloc_heal");
            alloc::run_vmalloc_heal();
        }
        Some(name) => {
            error!("Unknown MM test: {}", name);
            kprintln!(
                "Available MM tests: swiotlb, dma_coherent_guard, fork_cow, slub, buddy, page_counter_guard, status_readonly, pcp, heap, mmap, pt_ownership, pt_reclaim, vmalloc_heal"
            );
        }
    }

    mm_step("mm test suite done");
    kprintln!();
}

pub(super) fn pass(name: &str) {
    ok!("mm/{}", name);
}

pub(super) fn fail(name: &str, msg: &str) {
    MM_FAILED.store(true, Ordering::SeqCst);
    error!("mm/{}: {}", name, msg);
}

#[inline]
fn run_case(step: &str, case: fn()) {
    if MM_FAILED.load(Ordering::SeqCst) {
        return;
    }

    mm_step(step);
    case();

    if MM_FAILED.load(Ordering::SeqCst) {
        if crate::config::DEBUG_VERBOSE {
            kprintln!("[test/mm] abort remaining cases after failure");
        }
    }
}
