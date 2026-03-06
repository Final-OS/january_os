pub fn create_mmap_backing_for_pid(pid: usize, fd: i32) -> Result<u64, i32> {
    crate::fs::mmap_create_backing_for_pid(pid, fd)
}

pub fn retain_mmap_backing(backing_id: u64) -> Result<(), i32> {
    crate::fs::mmap_retain_backing(backing_id)
}

pub fn release_mmap_backing(backing_id: u64) {
    crate::fs::mmap_release_backing(backing_id)
}

pub fn copy_mmap_page(backing_id: u64, file_offset: usize, out: &mut [u8]) -> Result<usize, i32> {
    crate::fs::mmap_copy_page(backing_id, file_offset, out)
}
