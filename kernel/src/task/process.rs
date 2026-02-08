//! 进程抽象
//!
//! 当前阶段先提供最小进程描述，后续逐步接入地址空间、文件表、信号等。

use alloc::sync::Arc;
use alloc::string::String;
use alloc::vec::Vec;

use crate::sync::Mutex;

use super::id::ProcessId;
use super::task::Task;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessStatus {
    Running,
    Exiting,
    Zombie,
}

pub struct Process {
    pub pid: ProcessId,
    pub name: String,
    pub status: ProcessStatus,
    pub parent: Option<ProcessId>,
    pub tasks: Vec<Arc<Mutex<Task>>>,
    pub exit_code: Option<i32>,
}

impl Process {
    pub fn new_kernel(name: &str) -> Self {
        Self {
            pid: ProcessId::new(),
            name: String::from(name),
            status: ProcessStatus::Running,
            parent: None,
            tasks: Vec::new(),
            exit_code: None,
        }
    }

    pub fn add_task(&mut self, task: Arc<Mutex<Task>>) {
        self.tasks.push(task);
    }

    #[inline]
    pub fn task_count(&self) -> usize {
        self.tasks.len()
    }

    pub fn mark_exiting(&mut self, exit_code: i32) {
        self.status = ProcessStatus::Exiting;
        self.exit_code = Some(exit_code);
    }

    pub fn mark_zombie(&mut self) {
        self.status = ProcessStatus::Zombie;
    }
}
