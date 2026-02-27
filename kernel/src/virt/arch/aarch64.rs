use crate::virt::VirtInfo;

pub fn detect() -> VirtInfo {
    // 预留：后续通过 AArch64 HCR_EL2/SMCCC 等路径补齐探测。
    VirtInfo::bare_metal()
}
