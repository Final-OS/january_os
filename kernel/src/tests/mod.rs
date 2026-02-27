//! 内核测试框架
//!
//! 通过 shell `test <name>` 命令触发。

mod task;
mod libs;
mod mm;

use crate::{kprintln, ok, error};
use alloc::vec::Vec;

/// 执行测试子命令
pub fn run(name: &str) {
    // 解析命令：支持 "test libs rcu" 格式
    let parts: Vec<&str> = name.split_whitespace().collect();

    match parts.get(0).copied() {
        Some("task") => task::run(),
        Some("libs") => {
            let filter = parts.get(1).copied();
            libs::run_with_filter(filter);
        }
        Some("mm") => {
            let filter = parts.get(1).copied();
            mm::run_with_filter(filter);
        }
        Some("all") => {
            task::run();
            libs::run();
            mm::run();
        }
        Some("help") | _ => {
            kprintln!("Usage: test <subcommand>");
            kprintln!("Subcommands:");
            kprintln!("  task           - Kernel thread context switch");
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
