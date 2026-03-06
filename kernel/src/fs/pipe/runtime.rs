pub fn pipe2_for_pid(pid: usize, flags: u32) -> Result<(i32, i32), i32> {
    crate::fs::runtime::manager::pipe2_for_pid(pid, flags)
}
