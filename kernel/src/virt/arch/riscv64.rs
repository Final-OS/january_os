use crate::virt::VirtInfo;

pub fn detect() -> VirtInfo {
    // 预留：后续通过 RISC-V H 扩展能力探测补齐。
    VirtInfo::bare_metal()
}
