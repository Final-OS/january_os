pub mod heap;
pub mod slub;
pub mod vmalloc_heal;

pub(super) use super::{fail, mm_step, pass};

pub(super) fn run_heap() {
    heap::run();
}
pub(super) fn run_slub() {
    slub::run();
}
pub(super) fn run_vmalloc_heal() {
    vmalloc_heal::run();
}
