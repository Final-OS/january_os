use core::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcessId(pub usize);

pub type Pid = ProcessId;

impl TaskId {
    pub fn new() -> Self {
        static NEXT_TID: AtomicUsize = AtomicUsize::new(1);
        let tid = NEXT_TID
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                current.checked_add(1)
            })
            .expect("TaskId overflow");
        TaskId(tid)
    }
}

impl ProcessId {
    pub fn new() -> Self {
        static NEXT_PID: AtomicUsize = AtomicUsize::new(1);
        let pid = NEXT_PID
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                current.checked_add(1)
            })
            .expect("ProcessId overflow");
        ProcessId(pid)
    }
}
