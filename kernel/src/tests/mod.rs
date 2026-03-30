//! 内核测试框架
//!
//! 通过 shell `test <name>` 命令触发。

mod arch;
mod block;
mod drivers;
mod fs;
mod interrupt;
mod libs;
mod mm;
mod net;
mod security;
mod smp;
mod sync;
mod syscall;
mod task;
mod vfs;
mod virt;

use crate::kprintln;
use alloc::vec::Vec;

/// 执行测试子命令
pub fn run(name: &str) {
    // 解析命令：支持 "test libs rcu" 格式
    let parts: Vec<&str> = name.split_whitespace().collect();

    match parts.get(0).copied() {
        Some("task") => {
            let filter = parts.get(1).copied();
            task::run_with_filter(filter);
        }
        Some("arch") => arch::run(),
        Some("drivers") => drivers::run(),
        Some("interrupt") => interrupt::run(),
        Some("libs") => {
            let filter = parts.get(1).copied();
            libs::run_with_filter(filter);
        }
        Some("mm") => {
            let filter = parts.get(1).copied();
            mm::run_with_filter(filter);
        }
        Some("net") => net::run(),
        Some("security") => security::run(),
        Some("smp") => {
            let filter = parts.get(1).copied();
            smp::run_with_filter(filter);
        }
        Some("sync") => sync::run(),
        Some("syscall") => syscall::run(),
        Some("virt") => virt::run(),
        Some("fs") => {
            let filter = parts.get(1).copied();
            fs::run_with_filter(filter);
        }
        Some("block") => {
            let filter = parts.get(1).copied();
            block::run_with_filter(filter);
        }
        Some("vfs") => {
            let filter = parts.get(1).copied();
            vfs::run_with_filter(filter);
        }
        Some("all") => {
            arch::run();
            drivers::run();
            interrupt::run();
            task::run();
            libs::run();
            mm::run();
            net::run();
            security::run();
            smp::run();
            sync::run();
            syscall::run();
            fs::run();
            block::run();
            virt::run();
            vfs::run();
        }
        Some("help") | _ => {
            kprintln!("Usage: test <subcommand>");
            kprintln!("Subcommands:");
            kprintln!("  task [name]    - Task subsystem tests");
            kprintln!(
                "                   Available: switch, wait, usermode, regression, safe, all"
            );
            kprintln!("                   Default (`test task`) runs all");
            kprintln!("  arch           - Architecture skeleton tests");
            kprintln!("  drivers        - Driver skeleton tests");
            kprintln!("  interrupt      - Interrupt skeleton tests");
            kprintln!("  libs [name]    - Data structure tests");
            kprintln!("                   Available: rbtree, lru, rdtree, btree, mptree, rcu");
            kprintln!(
                "                              ring_buffer, kfifo, bitmap, hlist, wait_queue, id_allocator, sync_once, sync_blocking"
            );
            kprintln!("  mm [name]      - Memory management tests");
            kprintln!(
                "                   Available: swiotlb, dma_coherent_guard, slub, buddy, page_counter_guard, status_readonly, pcp, heap, mmap, pt_ownership, pt_reclaim, vmalloc_heal"
            );
            kprintln!("  smp [name]     - SMP/IPI tests");
            kprintln!(
                "                   Available: topology, cpu_id, ipi, irq_route, sched_stats, all"
            );
            kprintln!("  net            - Network skeleton tests");
            kprintln!("  security       - Security skeleton tests");
            kprintln!("  sync           - Synchronization skeleton tests");
            kprintln!("  syscall        - Syscall skeleton tests");
            kprintln!("  fs [name]      - File system tests");
            kprintln!(
                "                   Available: path, mount, fd_bridge, fd_ops, open_access, procfs, statfs, fat32, ext4"
            );
            kprintln!("  block [name]   - Block device tests");
            kprintln!("                   Available: virtio, virtio-scsi, partition");
            kprintln!("  virt           - Virtualization skeleton tests");
            kprintln!("  vfs [name]     - VFS core tests");
            kprintln!("                   Available: path, mount, fd_bridge");
            kprintln!("  all            - Run all tests");
        }
    }
}
