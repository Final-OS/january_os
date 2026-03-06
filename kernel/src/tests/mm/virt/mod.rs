pub mod fork_cow;
pub mod mmap;
pub mod pt_ownership;
pub mod pt_reclaim;

pub(super) use super::{fail, mm_step, pass};

pub(super) fn run_fork_cow() { fork_cow::run(); }
pub(super) fn run_mmap() { mmap::run(); }
pub(super) fn run_pt_ownership() { pt_ownership::run(); }
pub(super) fn run_pt_reclaim() { pt_reclaim::run(); }
