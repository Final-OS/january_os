#[derive(Debug, Clone, Copy)]
pub struct TaskState {
    pub scheduler_ready: bool,
    pub process_runtime_ready: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct TaskRuntimeState {
    pub ready: TaskState,
}

impl TaskRuntimeState {
    pub const fn placeholder() -> Self {
        Self {
            ready: TaskState {
                scheduler_ready: false,
                process_runtime_ready: false,
            },
        }
    }
}
