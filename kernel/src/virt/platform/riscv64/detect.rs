use crate::virt::core::info::VirtInfo;

pub fn detect() -> VirtInfo {
    VirtInfo::bare_metal()
}
