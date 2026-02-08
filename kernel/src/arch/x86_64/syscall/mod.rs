#[derive(Debug, Clone, Copy)]
pub struct SyscallFrame {
    pub nr: usize,
    pub arg0: usize,
    pub arg1: usize,
    pub arg2: usize,
    pub arg3: usize,
    pub arg4: usize,
    pub arg5: usize,
}

pub fn handle(frame: &SyscallFrame) -> usize {
    crate::syscall::dispatch(
        frame.nr,
        frame.arg0,
        frame.arg1,
        frame.arg2,
        frame.arg3,
        frame.arg4,
        frame.arg5,
    )
}
