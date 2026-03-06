use crate::virt::internal::handle::VirtHandle;

pub fn allocate() -> VirtHandle {
    VirtHandle(0)
}
