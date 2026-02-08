use core::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskId(pub usize);

pub type Pid = TaskId;
pub type ProcessId = TaskId;

impl TaskId {
    pub fn new() -> Self {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(1);
        TaskId(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }
}
