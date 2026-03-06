#[derive(Debug, Clone, Copy)]
pub struct TaskRuntimeRegistry {
    pub tasks_registered: usize,
    pub processes_registered: usize,
}

impl TaskRuntimeRegistry {
    pub const fn placeholder() -> Self {
        Self {
            tasks_registered: 0,
            processes_registered: 0,
        }
    }
}
