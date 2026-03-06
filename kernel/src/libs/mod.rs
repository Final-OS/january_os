//! 内核通用数据结构和库

use alloc::format;
use alloc::string::String;

use crate::component::{ComponentDescriptor, ComponentStage, ComponentStats};

pub mod bitmap;
pub mod btree;
pub mod hlist;
pub mod id_allocator;
pub mod kfifo;
pub mod list;
pub mod lru;
pub mod mptree;
pub mod rbtree;
pub mod rcu;
pub mod rdtree;
pub mod ring_buffer;
pub mod wait_queue;

pub const COMPONENT: ComponentDescriptor = ComponentDescriptor {
    id: "libs",
    stage: ComponentStage::Core,
    deps: &[],
    summary: "core data structures and utility collections",
};

pub fn init_early() -> crate::error::KernelResult<()> {
    Ok(())
}

pub fn init_core() -> crate::error::KernelResult<()> {
    Ok(())
}

pub fn init_late() -> crate::error::KernelResult<()> {
    Ok(())
}

pub fn stats() -> ComponentStats {
    ComponentStats::ready()
}

pub fn dump_state() -> String {
    format!("component={} state={:?}", COMPONENT.id, stats().state)
}
