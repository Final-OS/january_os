//! 进程抽象
//!
//! 当前阶段先提供最小进程描述，后续逐步接入地址空间、文件表、信号等。

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::sync::Mutex;

use crate::task::api::{ProcessId, TaskId};
use crate::task::proc::exec::ExecMappedPage;
use crate::task::thread::Task;

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
    pub mm: usize,
    pub is_clone_child: bool,
    pub name: String,
    pub last_exec_path: String,
    pub last_exec_argc: usize,
    pub last_exec_envc: usize,
    pub exec_request_seq: u64,
    pub status: ProcessStatus,
    pub parent: Option<ProcessId>,
    pub parent_tid: Option<TaskId>,
    pub children: Vec<ProcessId>,
    pub tasks: Vec<Arc<Mutex<Task>>>,
    pub exit_code: Option<i32>,
    pub wait_stop_signal: Option<i32>,
    pub wait_stop_reported: bool,
    pub wait_continued_pending: bool,
    pub exec_mappings: Vec<ExecMappedPage>,
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
            mm: crate::mm::init_mm_ptr() as usize,
            is_clone_child: false,
            name: String::from(name),
            last_exec_path: String::from(name),
            last_exec_argc: 0,
            last_exec_envc: 0,
            exec_request_seq: 0,
            status: ProcessStatus::Running,
            parent,
            parent_tid,
            children: Vec::new(),
            tasks: Vec::new(),
            exit_code: None,
            wait_stop_signal: None,
            wait_stop_reported: false,
            wait_continued_pending: false,
            exec_mappings: Vec::new(),
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

    pub fn replace_exec_mappings(&mut self, mappings: Vec<ExecMappedPage>) -> Vec<ExecMappedPage> {
        core::mem::replace(&mut self.exec_mappings, mappings)
    }

    pub fn take_exec_mappings(&mut self) -> Vec<ExecMappedPage> {
        core::mem::take(&mut self.exec_mappings)
    }

    #[inline]
    pub fn exec_mapping_count(&self) -> usize {
        self.exec_mappings.len()
    }

    pub fn record_exec_request(&mut self, path: &str, argc: usize, envc: usize) {
        self.last_exec_path = String::from(path);
        self.last_exec_argc = argc;
        self.last_exec_envc = envc;
        self.exec_request_seq = self.exec_request_seq.saturating_add(1);
        self.name = String::from(path);
        self.status = ProcessStatus::Running;
        self.wait_stop_signal = None;
        self.wait_stop_reported = false;
        self.wait_continued_pending = false;
    }
}
