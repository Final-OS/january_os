//! 内核测试框架
//!
//! 通过 shell `test <name>` 命令触发。

mod task;
mod libs;
mod mm;

use crate::kprintln;
use alloc::vec::Vec;

/// 执行测试子命令
pub fn run(name: &str) {
    // 解析命令：支持 "test libs rcu" 格式
    let parts: Vec<&str> = name.split_whitespace().collect();
    kprintln!("[test] request='{}' argc={}", name, parts.len());

    match parts.get(0).copied() {
        Some("task") => {
            let filter = parts.get(1).copied();
            kprintln!("[test] dispatch module=task filter={:?}", filter);
            task::run_with_filter(filter);
        }
        Some("libs") => {
            let filter = parts.get(1).copied();
            kprintln!("[test] dispatch module=libs filter={:?}", filter);
            libs::run_with_filter(filter);
        }
        Some("mm") => {
            let filter = parts.get(1).copied();
            kprintln!("[test] dispatch module=mm filter={:?}", filter);
            mm::run_with_filter(filter);
        }
        Some("all") => {
            kprintln!("[test] dispatch module=all (task+libs+mm)");
            task::run();
            libs::run();
            mm::run();
        }
        Some("help") | _ => {
            kprintln!("[test] dispatch module=help");
            kprintln!("Usage: test <subcommand>");
            kprintln!("Subcommands:");
            kprintln!("  task [name]    - Task subsystem tests");
            kprintln!("                   Available: switch, wait, usermode, regression, safe, all");
            kprintln!("                   Default (`test task`) runs all");
            kprintln!("  libs [name]    - Data structure tests");
            kprintln!("                   Available: rbtree, lru, rdtree, btree, mptree, rcu");
            kprintln!("                              ring_buffer, kfifo, bitmap, hlist, wait_queue, id_allocator");
            kprintln!("  mm [name]      - Memory management tests");
            kprintln!("                   Available: swiotlb, slub, buddy, pcp");
            kprintln!("  timer          - Timer tick test");
            kprintln!("  all            - Run all tests");
        }
    }
}
