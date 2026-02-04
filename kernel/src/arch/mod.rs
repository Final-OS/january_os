//! 架构抽象模块
//!
//! 根据目标架构选择对应的实现。

#[cfg(target_arch = "x86_64")]
pub mod x86_64;

#[cfg(target_arch = "x86_64")]
pub use x86_64::*;
