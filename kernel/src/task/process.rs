//! 进程抽象
//!
//! 当前阶段先提供最小进程描述，后续逐步接入地址空间、文件表、信号等。

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::sync::Mutex;

use super::id::{ProcessId, TaskId};
use super::task::Task;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessStatus {
    Running,
    Stopped,
    Exiting,
    Zombie,
}

pub struct Process {
    pub pid: ProcessId,
    pub pgid: ProcessId,
    pub is_clone_child: bool,
    pub name: String,
    pub status: ProcessStatus,
    pub parent: Option<ProcessId>,
    pub parent_tid: Option<TaskId>,
    pub children: Vec<ProcessId>,
    pub tasks: Vec<Arc<Mutex<Task>>>,
    pub exit_code: Option<i32>,
    pub wait_stop_signal: Option<i32>,
    pub wait_stop_reported: bool,
    pub wait_continued_pending: bool,
}

impl Process {
    pub fn new_kernel(name: &str) -> Self {
        let pid = ProcessId::new();
        Self::new_kernel_with_pid(name, pid, pid, None, None)
    }

    pub fn new_kernel_with_pid(
        name: &str,
        pid: ProcessId,
        pgid: ProcessId,
        parent: Option<ProcessId>,
        parent_tid: Option<TaskId>,
    ) -> Self {
        Self {
            pid,
            pgid,
            is_clone_child: false,
            name: String::from(name),
            status: ProcessStatus::Running,
            parent,
            parent_tid,
            children: Vec::new(),
            tasks: Vec::new(),
            exit_code: None,
            wait_stop_signal: None,
            wait_stop_reported: false,
            wait_continued_pending: false,
        }
    }

    pub fn add_task(&mut self, task: Arc<Mutex<Task>>) {
        self.tasks.push(task);
    }

    #[inline]
    pub fn task_count(&self) -> usize {
        self.tasks.len()
    }

    pub fn add_child(&mut self, child_pid: ProcessId) {
        if self.children.iter().any(|pid| *pid == child_pid) {
            return;
        }
        self.children.push(child_pid);
    }

    pub fn remove_child(&mut self, child_pid: ProcessId) -> bool {
        if let Some(index) = self.children.iter().position(|pid| *pid == child_pid) {
            self.children.remove(index);
            true
        } else {
            false
        }
    }

    pub fn mark_exiting(&mut self, exit_code: i32) {
        self.status = ProcessStatus::Exiting;
        self.exit_code = Some(exit_code);
        self.wait_stop_signal = None;
        self.wait_stop_reported = false;
        self.wait_continued_pending = false;
    }

    pub fn mark_zombie(&mut self) {
        self.status = ProcessStatus::Zombie;
        self.wait_stop_signal = None;
        self.wait_stop_reported = false;
        self.wait_continued_pending = false;
    }

    pub fn mark_stopped(&mut self, signal: i32) {
        self.status = ProcessStatus::Stopped;
        self.wait_stop_signal = Some(signal);
        self.wait_stop_reported = false;
        self.wait_continued_pending = false;
    }

    pub fn mark_continued(&mut self) {
        self.status = ProcessStatus::Running;
        self.wait_stop_signal = None;
        self.wait_stop_reported = false;
        self.wait_continued_pending = true;
    }

    pub fn mark_wait_stopped_reported(&mut self) {
        self.wait_stop_reported = true;
    }

    pub fn clear_wait_continued_pending(&mut self) {
        self.wait_continued_pending = false;
    }
}
