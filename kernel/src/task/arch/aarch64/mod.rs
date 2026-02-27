//! AArch64 任务架构占位实现

#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct TaskContext {
    pub sp: u64,
}

/// AArch64 用户态入口帧占位。
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct UserEnterFrame {
    pub pc: u64,
    pub sp: u64,
    pub pstate: u64,
}

pub fn build_user_enter_frame(entry: u64, stack_top: u64) -> UserEnterFrame {
    UserEnterFrame {
        pc: entry,
        sp: stack_top,
        pstate: 0,
    }
}

pub unsafe fn enter_user_mode_iret(_frame: &UserEnterFrame) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

pub unsafe extern "C" fn __switch(
    _current_task_cx_ptr: *mut usize,
    _next_task_cx_ptr: *const usize,
) {
}
