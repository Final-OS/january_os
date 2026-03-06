use crate::virt::error::VirtResult;
use crate::virt::memory::mmio;
use crate::virt::types::MmioRegion;

pub fn register_default_regions() -> VirtResult<()> {
    mmio::register_region(0xfee0_0000, 0x1000)
}

pub fn validate_region(region: MmioRegion) -> VirtResult<()> {
    mmio::validate_region(region)
}
