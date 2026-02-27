//! 内存管理子系统测试

mod buddy;
mod pcp;
mod slub;
mod swiotlb;

use crate::{error, kprintln, ok};
use core::sync::atomic::{AtomicUsize, Ordering};

static MM_STEP_SEQ: AtomicUsize = AtomicUsize::new(0);

pub(super) fn mm_step(msg: &str) {
    let seq = MM_STEP_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    kprintln!("[test/mm][step {}] {}", seq, msg);
}

pub fn run() {
    run_with_filter(None);
}

pub fn run_with_filter(filter: Option<&str>) {
    MM_STEP_SEQ.store(0, Ordering::SeqCst);
    kprintln!("=== MM Subsystem Tests ===");
    mm_step("start mm test suite");
    kprintln!("[test/mm] filter={:?}", filter);

    match filter {
        None | Some("all") => {
            mm_step("run case=swiotlb");
            swiotlb::run();
            mm_step("run case=slub");
            slub::run();
            mm_step("run case=buddy");
            buddy::run();
            mm_step("run case=pcp");
            pcp::run();
        }
        Some("swiotlb") => {
            mm_step("run case=swiotlb");
            swiotlb::run();
        }
        Some("slub") => {
            mm_step("run case=slub");
            slub::run();
        }
        Some("buddy") => {
            mm_step("run case=buddy");
            buddy::run();
        }
        Some("pcp") => {
            mm_step("run case=pcp");
            pcp::run();
        }
        Some(name) => {
            error!("Unknown MM test: {}", name);
            kprintln!("Available MM tests: swiotlb, slub, buddy, pcp");
        }
    }

    mm_step("mm test suite done");
    kprintln!();
}

pub(super) fn pass(name: &str) {
    ok!("mm/{}", name);
}

pub(super) fn fail(name: &str, msg: &str) {
    error!("mm/{}: {}", name, msg);
}
