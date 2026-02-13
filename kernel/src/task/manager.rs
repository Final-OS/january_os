//! 任务管理器
//!
//! 负责任务的创建、销毁和查找。

use super::id::{ProcessId, TaskId};
use super::process::{Process, ProcessStatus};
use super::scheduler::SCHEDULER;
use super::task::{Task, TaskStatus};
use crate::libs::rdtree::RadixTree;
use crate::sync::Mutex;
use alloc::sync::Arc;
use alloc::vec::Vec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitChildResult {
    Reaped(ProcessId, i32),
    NoMatchedChild,
    ChildRunning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitTarget {
    Any,
    Pid(ProcessId),
    Pgid(ProcessId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitCloneFilter {
    NonCloneOnly,
    CloneOnly,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaitChildOptions {
    pub include_stopped: bool,
    pub include_continued: bool,
    pub clone_filter: WaitCloneFilter,
    pub current_thread_only: bool,
}

impl Default for WaitChildOptions {
    fn default() -> Self {
        Self {
            include_stopped: false,
            include_continued: false,
            clone_filter: WaitCloneFilter::NonCloneOnly,
            current_thread_only: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WaitRusageSnapshot {
    pub user_ticks: u64,
    pub system_ticks: u64,
    pub minor_faults: u64,
    pub major_faults: u64,
    pub inblock: u64,
    pub oublock: u64,
    pub signals_delivered: u64,
    pub voluntary_ctxt_switches: u64,
    pub involuntary_ctxt_switches: u64,
    pub max_rss_kb: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitChildConsumeEvent {
    Stopped,
    Continued,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitChildObserveResult {
    Reapable(ProcessId, i32),
    Stopped(ProcessId, i32),
    Continued(ProcessId),
    NoMatchedChild,
    ChildRunning,
}

/// 全局任务管理器
static TASK_MANAGER: Mutex<TaskManager> = Mutex::new(TaskManager::new());

pub struct TaskManager {
    tasks: RadixTree<Arc<Mutex<Task>>>,
    processes: RadixTree<Arc<Mutex<Process>>>,
}

impl TaskManager {
    pub const fn new() -> Self {
        Self {
            tasks: RadixTree::new(),
            processes: RadixTree::new(),
        }
    }

    pub fn add_task(&mut self, task: Arc<Mutex<Task>>) {
        let tid = task.lock().id.0;
        self.tasks.insert(tid, task);
    }

    pub fn add_process(&mut self, process: Arc<Mutex<Process>>) {
        let pid = process.lock().pid.0;
        self.processes.insert(pid, process);
    }

    pub fn find_task_by_pid(&self, pid: ProcessId) -> Option<&Arc<Mutex<Task>>> {
        self.tasks
            .iter()
            .find_map(|(_tid, task)| (task.lock().pid == pid).then_some(task))
    }

    pub fn find_process_by_pid(&self, pid: ProcessId) -> Option<&Arc<Mutex<Process>>> {
        self.processes.get(pid.0)
    }

    pub fn remove_task_by_tid(&mut self, tid: TaskId) -> Option<Arc<Mutex<Task>>> {
        self.tasks.remove(tid.0)
    }

    pub fn remove_tasks_by_process(&mut self, pid: ProcessId) -> usize {
        let tids: Vec<usize> = self
            .tasks
            .iter()
            .filter_map(|(tid, task)| (task.lock().pid == pid).then_some(tid))
            .collect();

        for tid in tids.iter().copied() {
            let _ = self.remove_task_by_tid(TaskId(tid));
        }

        tids.len()
    }

    pub fn remove_process_by_pid(&mut self, pid: ProcessId) -> Option<Arc<Mutex<Process>>> {
        self.processes.remove(pid.0)
    }

    #[allow(dead_code)]
    pub fn task_count(&self) -> usize {
        self.tasks.len()
    }

    #[allow(dead_code)]
    pub fn process_count(&self) -> usize {
        self.processes.len()
    }
}

pub fn init() {
    // 可以在这里做一些初始化工作
}

/// 根据 PID 查找任务
pub fn find_task_by_pid(pid: ProcessId) -> Option<Arc<Mutex<Task>>> {
    TASK_MANAGER.lock().find_task_by_pid(pid).cloned()
}

/// 根据 PID 查找进程
pub fn find_process_by_pid(pid: ProcessId) -> Option<Arc<Mutex<Process>>> {
    TASK_MANAGER.lock().find_process_by_pid(pid).cloned()
}

fn child_matches_target(child_pid: ProcessId, target: WaitTarget) -> bool {
    match target {
        WaitTarget::Any => true,
        WaitTarget::Pid(target_pid) => child_pid == target_pid,
        WaitTarget::Pgid(target_pgid) => find_process_by_pid(child_pid)
            .map(|process| process.lock().pgid == target_pgid)
            .unwrap_or(false),
    }
}

fn child_matches_clone_filter(child_pid: ProcessId, clone_filter: WaitCloneFilter) -> bool {
    let Some(process) = find_process_by_pid(child_pid) else {
        return false;
    };

    let process = process.lock();
    match clone_filter {
        WaitCloneFilter::All => true,
        WaitCloneFilter::NonCloneOnly => !process.is_clone_child,
        WaitCloneFilter::CloneOnly => process.is_clone_child,
    }
}

fn child_matches_thread_scope(
    child_pid: ProcessId,
    current_tid: TaskId,
    current_thread_only: bool,
) -> bool {
    if !current_thread_only {
        return true;
    }

    let Some(process) = find_process_by_pid(child_pid) else {
        return false;
    };

    let process = process.lock();
    match process.parent_tid {
        Some(parent_tid) => parent_tid == current_tid,
        None => true,
    }
}

fn parent_owns_child(child_pid: ProcessId) -> Option<(ProcessId, Arc<Mutex<Process>>)> {
    let parent_pid = super::processor::current_task().map(|task| task.lock().pid)?;
    let parent_process = find_process_by_pid(parent_pid)?;

    let owns_child = {
        let parent = parent_process.lock();
        parent.children.iter().any(|pid| *pid == child_pid)
    };

    if !owns_child {
        return None;
    }

    Some((parent_pid, parent_process))
}

pub fn wait_child_observe_by_target_with_options(
    target: WaitTarget,
    options: WaitChildOptions,
) -> WaitChildObserveResult {
    let (parent_pid, current_tid) = match super::processor::current_task() {
        Some(task) => {
            let task = task.lock();
            (task.pid, task.id)
        }
        None => return WaitChildObserveResult::NoMatchedChild,
    };

    let parent_process = match find_process_by_pid(parent_pid) {
        Some(process) => process,
        None => return WaitChildObserveResult::NoMatchedChild,
    };

    let candidate_children: Vec<ProcessId> = {
        let parent = parent_process.lock();
        parent
            .children
            .iter()
            .copied()
            .filter(|child_pid| {
                child_matches_target(*child_pid, target)
                    && child_matches_clone_filter(*child_pid, options.clone_filter)
                    && child_matches_thread_scope(
                        *child_pid,
                        current_tid,
                        options.current_thread_only,
                    )
            })
            .collect()
    };

    if candidate_children.is_empty() {
        return WaitChildObserveResult::NoMatchedChild;
    }

    for child_pid in candidate_children {
        if let Some(process) = find_process_by_pid(child_pid) {
            let process = process.lock();
            if process.status == ProcessStatus::Zombie {
                let exit_code = process.exit_code.unwrap_or(0);
                return WaitChildObserveResult::Reapable(child_pid, exit_code);
            }

            if options.include_stopped
                && process.status == ProcessStatus::Stopped
                && !process.wait_stop_reported
            {
                let signal = process.wait_stop_signal.unwrap_or(19);
                return WaitChildObserveResult::Stopped(child_pid, signal);
            }

            if options.include_continued && process.wait_continued_pending {
                return WaitChildObserveResult::Continued(child_pid);
            }
        }
    }

    WaitChildObserveResult::ChildRunning
}

pub fn wait_child_observe_by_target(target: WaitTarget) -> WaitChildObserveResult {
    wait_child_observe_by_target_with_options(target, WaitChildOptions::default())
}

pub fn reap_observed_child(child_pid: ProcessId) -> Option<(ProcessId, i32)> {
    let (parent_pid, parent_process) = parent_owns_child(child_pid)?;

    let exit_code = {
        let child_process = find_process_by_pid(child_pid)?;
        let child = child_process.lock();
        if child.status != ProcessStatus::Zombie {
            return None;
        }
        child.exit_code.unwrap_or(0)
    };

    {
        let mut parent = parent_process.lock();
        parent.remove_child(child_pid);
    }

    {
        let mut manager = TASK_MANAGER.lock();
        let _ = manager.remove_process_by_pid(child_pid)?;
        let _ = manager.remove_tasks_by_process(child_pid);
    }

    let removed_ready = SCHEDULER.lock().remove_tasks_by_pid(child_pid);

    crate::kprintln!(
        "[diag][task] reap child: parent_pid={} child_pid={} code={} removed_ready={}",
        parent_pid.0,
        child_pid.0,
        exit_code,
        removed_ready
    );

    Some((child_pid, exit_code))
}

pub fn consume_observed_wait_event(child_pid: ProcessId, event: WaitChildConsumeEvent) -> bool {
    let (_, _parent_process) = match parent_owns_child(child_pid) {
        Some(pair) => pair,
        None => return false,
    };

    let Some(child_process_ref) = find_process_by_pid(child_pid) else {
        return false;
    };

    let mut child_process = child_process_ref.lock();
    match event {
        WaitChildConsumeEvent::Stopped => {
            if child_process.status != ProcessStatus::Stopped || child_process.wait_stop_reported {
                return false;
            }
            child_process.mark_wait_stopped_reported();
            true
        }
        WaitChildConsumeEvent::Continued => {
            if !child_process.wait_continued_pending {
                return false;
            }
            child_process.clear_wait_continued_pending();
            true
        }
    }
}

pub fn snapshot_observed_child_rusage(child_pid: ProcessId) -> Option<WaitRusageSnapshot> {
    let _ = parent_owns_child(child_pid)?;

    let child_process_ref = find_process_by_pid(child_pid)?;
    let now_ticks = crate::interrupt::timer_ticks();
    let child_process = child_process_ref.lock();

    let mut snapshot = WaitRusageSnapshot::default();

    for task_ref in child_process.tasks.iter() {
        let task = task_ref.lock();
        snapshot.system_ticks = snapshot
            .system_ticks
            .saturating_add(task.total_runtime_ticks(now_ticks));
        snapshot.voluntary_ctxt_switches = snapshot
            .voluntary_ctxt_switches
            .saturating_add(task.voluntary_switches);
        snapshot.involuntary_ctxt_switches = snapshot
            .involuntary_ctxt_switches
            .saturating_add(task.involuntary_switches);
    }

    Some(snapshot)
}

/// 创建内核线程并添加到调度器
pub fn spawn_kernel_thread(name: &str, entry: extern "C" fn()) -> Arc<Mutex<Task>> {
    let (parent_pid, parent_tid) = match super::processor::current_task() {
        Some(task) => {
            let task = task.lock();
            (Some(task.pid), Some(task.id))
        }
        None => (None, None),
    };
    let pid = ProcessId::new();
    let ppid = parent_pid.unwrap_or(ProcessId(0));
    let pgid = parent_pid
        .and_then(|ppid| find_process_by_pid(ppid).map(|process| process.lock().pgid))
        .unwrap_or(pid);

    let task = Task::new_kernel_for_process(name, entry, pid, ppid);
    let task_ref = Arc::new(Mutex::new(task));

    let mut process = Process::new_kernel_with_pid(name, pid, pgid, parent_pid, parent_tid);
    process.add_task(task_ref.clone());
    let process_ref = Arc::new(Mutex::new(process));

    {
        let mut manager = TASK_MANAGER.lock();
        manager.add_process(process_ref);
        manager.add_task(task_ref.clone());
    }

    if let Some(parent_pid) = parent_pid {
        if let Some(parent_process) = find_process_by_pid(parent_pid) {
            parent_process.lock().add_child(pid);
        }
    }

    crate::kprintln!(
        "[diag][task] spawn kernel thread: pid={} pgid={} ppid={} name={}",
        pid.0,
        pgid.0,
        ppid.0,
        name
    );

    // 添加到就绪队列
    SCHEDULER.lock().add_task(task_ref.clone());

    task_ref
}

/// 退出当前任务
pub fn exit_current_task(exit_code: i32) {
    let Some(task_ref) = super::processor::current_task() else {
        return;
    };

    let pid = {
        let mut task = task_ref.lock();
        task.status = TaskStatus::Exited;
        task.exit_code = Some(exit_code);
        crate::kprintln!(
            "[diag][task] task exit: tid={} pid={} code={}",
            task.id.0,
            task.pid.0,
            exit_code
        );
        task.pid
    };

    if let Some(process_ref) = find_process_by_pid(pid) {
        let mut process = process_ref.lock();
        let all_exited = process
            .tasks
            .iter()
            .all(|task| task.lock().status == TaskStatus::Exited);

        if all_exited {
            process.mark_exiting(exit_code);
            process.mark_zombie();
            crate::kprintln!(
                "[diag][task] process became zombie: pid={} code={}",
                pid.0,
                exit_code
            );
        }
    }
}

/// 退出当前进程（退出其所有任务）
pub fn exit_current_process(exit_code: i32) {
    let Some(task_ref) = super::processor::current_task() else {
        return;
    };

    let pid = {
        let task = task_ref.lock();
        task.pid
    };

    let Some(process_ref) = find_process_by_pid(pid) else {
        let mut task = task_ref.lock();
        task.status = TaskStatus::Exited;
        task.exit_code = Some(exit_code);
        return;
    };

    let tasks = {
        let process = process_ref.lock();
        process.tasks.clone()
    };
    let task_count = tasks.len();

    for task_ref in tasks {
        let mut task = task_ref.lock();
        task.status = TaskStatus::Exited;
        task.exit_code = Some(exit_code);
    }

    let mut process = process_ref.lock();
    process.mark_exiting(exit_code);
    process.mark_zombie();

    // 清理同进程残留的就绪队列项，避免退出任务再次被调度。
    let removed_ready = SCHEDULER.lock().remove_tasks_by_pid(pid);
    crate::kprintln!(
        "[diag][task] exit_group: pid={} code={} tasks={} removed_ready={}",
        pid.0,
        exit_code,
        task_count,
        removed_ready
    );
}

/// 等待子进程退出（按目标回收 Zombie 子进程）
pub fn wait_child_result_by_target(target: WaitTarget) -> WaitChildResult {
    match wait_child_observe_by_target_with_options(target, WaitChildOptions::default()) {
        WaitChildObserveResult::Reapable(pid, _exit_code) => {
            match reap_observed_child(pid) {
                Some((reaped_pid, exit_code)) => WaitChildResult::Reaped(reaped_pid, exit_code),
                None => WaitChildResult::ChildRunning,
            }
        }
        WaitChildObserveResult::Stopped(_, _) | WaitChildObserveResult::Continued(_) => {
            WaitChildResult::ChildRunning
        }
        WaitChildObserveResult::NoMatchedChild => WaitChildResult::NoMatchedChild,
        WaitChildObserveResult::ChildRunning => WaitChildResult::ChildRunning,
    }
}

/// 等待子进程退出（最小实现：回收 Zombie 子进程）
pub fn wait_child_result(target_pid: Option<ProcessId>) -> WaitChildResult {
    let target = match target_pid {
        Some(pid) => WaitTarget::Pid(pid),
        None => WaitTarget::Any,
    };

    wait_child_result_by_target(target)
}

/// 等待子进程退出（兼容旧接口）
pub fn wait_child(target_pid: Option<ProcessId>) -> Option<(ProcessId, i32)> {
    match wait_child_result(target_pid) {
        WaitChildResult::Reaped(pid, exit_code) => Some((pid, exit_code)),
        WaitChildResult::NoMatchedChild | WaitChildResult::ChildRunning => None,
    }
}
