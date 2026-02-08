//! 内核测试框架
//!
//! 通过 shell `test <name>` 命令触发。

mod task_test;
mod libs_test;

use crate::{kprintln, ok, error};

/// 执行测试子命令
pub fn run(name: &str) {
    match name {
        "task" => task_test::run(),
        "libs" => libs_test::run(),
        "all" => {
            task_test::run();
            libs_test::run();
        }
        "help" | _ => {
            kprintln!("Usage: test <subcommand>");
            kprintln!("Subcommands:");
            kprintln!("  task      - Kernel thread context switch");
            kprintln!("  libs      - Data structure tests");
            kprintln!("  timer     - Timer tick test");
            kprintln!("  all       - Run all tests");
        }
    }
}
