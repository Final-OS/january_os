//! 内核数据结构 (libs) 测试

mod btree;
mod collections;
mod lru;
mod maple;
mod radix;
mod rbtree;
mod rcu;

use crate::{error, kprintln, ok};
use core::sync::atomic::{AtomicUsize, Ordering};

static LIBS_STEP_SEQ: AtomicUsize = AtomicUsize::new(0);

fn libs_step(msg: &str) {
    let seq = LIBS_STEP_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    if crate::config::DEBUG_VERBOSE {
        kprintln!("[test/libs][step {}] {}", seq, msg);
    }
}

fn run_case(name: &str, f: fn()) {
    libs_step("case begin");
    if crate::config::DEBUG_VERBOSE {
        kprintln!("[test/libs] case={} begin", name);
    }
    f();
    if crate::config::DEBUG_VERBOSE {
        kprintln!("[test/libs] case={} end", name);
    }
}

pub fn run() {
    run_with_filter(None);
}

pub fn run_with_filter(filter: Option<&str>) {
    LIBS_STEP_SEQ.store(0, Ordering::SeqCst);
    kprintln!("=== Libs Data Structure Tests ===");
    libs_step("start libs test suite");
    if crate::config::DEBUG_VERBOSE {
        kprintln!("[test/libs] filter={:?}", filter);
    }

    match filter {
        None | Some("all") => {
            run_case("rbtree", rbtree::run);
            run_case("lru", lru::run);
            run_case("rdtree", radix::run);
            run_case("btree", btree::run);
            run_case("mptree", maple::run);
            run_case("rcu", rcu::run);
            run_case("ring_buffer", collections::run_ring_buffer);
            run_case("kfifo", collections::run_kfifo);
            run_case("bitmap", collections::run_bitmap);
            run_case("hlist", collections::run_hlist);
            run_case("wait_queue", collections::run_wait_queue);
            run_case("id_allocator", collections::run_id_allocator);
            run_case("sync_once", collections::run_sync_once);
            run_case("sync_blocking", collections::run_sync_blocking);
        }
        Some("rbtree") => run_case("rbtree", rbtree::run),
        Some("lru") => run_case("lru", lru::run),
        Some("rdtree") | Some("radix") => run_case("rdtree", radix::run),
        Some("btree") => run_case("btree", btree::run),
        Some("mptree") | Some("maple") => run_case("mptree", maple::run),
        Some("rcu") => run_case("rcu", rcu::run),
        Some("ring") | Some("ring_buffer") => run_case("ring_buffer", collections::run_ring_buffer),
        Some("kfifo") => run_case("kfifo", collections::run_kfifo),
        Some("bitmap") => run_case("bitmap", collections::run_bitmap),
        Some("hlist") => run_case("hlist", collections::run_hlist),
        Some("waitq") | Some("wait_queue") => run_case("wait_queue", collections::run_wait_queue),
        Some("idalloc") | Some("id_allocator") => {
            run_case("id_allocator", collections::run_id_allocator)
        }
        Some("once") | Some("sync_once") => run_case("sync_once", collections::run_sync_once),
        Some("sync_blocking") => run_case("sync_blocking", collections::run_sync_blocking),
        Some(name) => {
            error!("Unknown test: {}", name);
            kprintln!("Available tests: rbtree, lru, rdtree, btree, mptree, rcu, ring_buffer, kfifo, bitmap, hlist, wait_queue, id_allocator, sync_once, sync_blocking");
        }
    }

    libs_step("libs test suite done");
    kprintln!();
}

pub(super) fn pass(name: &str) {
    ok!("libs/{}", name);
}

pub(super) fn fail(name: &str, msg: &str) {
    error!("libs/{}: {}", name, msg);
}
