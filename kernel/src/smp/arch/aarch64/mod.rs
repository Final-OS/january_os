//! SMP AArch64 占位实现。

pub unsafe fn prepare_smp<T>(_madt_like: &T, _direct_map_base: u64) {}

pub fn boot_ap(_cpu_id: u32, _direct_map_base: u64) {}
