//! january_os 内核库
//!
//! 这是内核的主要库，包含所有内核模块。

#![no_std]
#![feature(alloc_error_handler)]
#![feature(abi_x86_interrupt)]
// 开发阶段允许的警告（后续逐步修复）
#![allow(dead_code)]
#![allow(unused)]
#![allow(private_interfaces)]
#![allow(non_camel_case_types)]
#![allow(mismatched_lifetime_syntaxes)]
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(function_casts_as_integer)]
#![allow(clippy::all)]

// 自动生成的配置 (从 os_cfg.toml 生成)
mod generated;
pub mod config {
    pub use super::generated::*;
}

// 模块声明
pub mod error;
pub mod arch;
pub mod drivers;
pub mod interrupt;
#[macro_use]
pub mod mm;
pub mod sync;

// 兼容性：重新导出 acpi (现在在 drivers 下)
pub use drivers::acpi;

// 重新导出常用类型
pub use mm::{PhysAddr, VirtAddr};
