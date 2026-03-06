//! 虚拟内存管理
//!
//! 包含地址定义、页表管理、VMA、缺页处理等。

pub use crate::mm::api::layout;
pub use crate::mm::api::layout::*;

pub mod address;
pub use address::*;

pub mod paging;
pub use paging::*;

pub mod vma;
pub use vma::*;

pub mod fault;
pub use fault::*;

pub mod layout_runtime;
pub use layout_runtime::*;
