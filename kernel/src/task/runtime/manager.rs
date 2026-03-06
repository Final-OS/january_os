//! 任务管理器
//!
//! 负责任务的创建、销毁和查找。

use crate::task::api::{ProcessId, TaskId};
use crate::task::proc::exec::{rollback_exec_mappings, ExecMappedPage};
use crate::task::proc::{Process, ProcessStatus};
use crate::task::sched::SCHEDULER;
use crate::task::thread::{Task, TaskStatus};
use crate::fs;
use crate::libs::rdtree::RadixTree;
use crate::sync::Mutex;
use alloc::string::String;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnMmMode {
    InheritShared,
    InheritPrivate,
    InheritInit,
    InheritInitPrivate,
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

    pub fn find_task_by_tid(&self, tid: TaskId) -> Option<&Arc<Mutex<Task>>> {
        self.tasks.get(tid.0)
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

    pub fn all_process_ids(&self) -> Vec<ProcessId> {
        self.processes
            .iter()
            .map(|(pid, _)| ProcessId(pid))
            .collect()
    }

    pub fn process_ids_by_pgid(&self, pgid: ProcessId) -> Vec<ProcessId> {
        self.processes
            .iter()
            .filter_map(|(pid, process)| (process.lock().pgid == pgid).then_some(ProcessId(pid)))
            .collect()
    }
}

pub fn init() {
    // 可以在这里做一些初始化工作
}

#[inline]
fn resolve_child_mm(parent_pid: Option<ProcessId>, mm_mode: SpawnMmMode) -> Option<usize> {
    let parent_mm = parent_pid
        .and_then(|ppid| {
            find_process_by_pid(ppid).map(|process| process.lock().mm as *mut crate::mm::Mm)
        })
        .unwrap_or(crate::mm::init_mm_ptr());

    match mm_mode {
        SpawnMmMode::InheritShared => Some(crate::mm::mm_retain(parent_mm) as usize),
        SpawnMmMode::InheritPrivate => {
            let cloned = crate::mm::mm_clone(parent_mm);
            (!cloned.is_null()).then_some(cloned as usize)
        }
        SpawnMmMode::InheritInit => Some(crate::mm::mm_retain(crate::mm::init_mm_ptr()) as usize),
        SpawnMmMode::InheritInitPrivate => {
            let cloned = crate::mm::mm_clone(crate::mm::init_mm_ptr());
            (!cloned.is_null()).then_some(cloned as usize)
        }
    }
}

/// 获取当前执行上下文对应的地址空间指针
///
/// 当前阶段仍可能回退到 `init_mm`，后续可平滑切换到真正的 per-process mm。
pub fn current_mm_ptr() -> *mut crate::mm::Mm {
    let Some(task_ref) = crate::task::thread::current_task() else {
        return crate::mm::init_mm_ptr();
    };

    let pid = {
        let task = task_ref.lock();
        task.pid
    };

    let Some(process_ref) = find_process_by_pid(pid) else {
        return crate::mm::init_mm_ptr();
    };

    let mm_ptr = {
        let process = process_ref.lock();
        process.mm as *mut crate::mm::Mm
    };

    if mm_ptr.is_null() {
        crate::mm::init_mm_ptr()
    } else {
        mm_ptr
    }
}

/// 根据 PID 查找任务
pub fn find_task_by_pid(pid: ProcessId) -> Option<Arc<Mutex<Task>>> {
    TASK_MANAGER.lock().find_task_by_pid(pid).cloned()
}

/// 根据 PID 查找进程
pub fn find_process_by_pid(pid: ProcessId) -> Option<Arc<Mutex<Process>>> {
    TASK_MANAGER.lock().find_process_by_pid(pid).cloned()
}

pub fn record_current_exec_request(path: &str, argc: usize, envc: usize) -> Option<ProcessId> {
    let current_task = crate::task::thread::current_task()?;

    let pid = {
        let mut task = current_task.lock();
        task.name = String::from(path);
        task.pid
    };

    let process_ref = find_process_by_pid(pid)?;

    let (seq, pgid) = {
        let mut process = process_ref.lock();
        process.record_exec_request(path, argc, envc);
        (process.exec_request_seq, process.pgid)
    };

    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[execve] request recorded: pid={} pgid={} path={} argc={} envc={} seq={}",
            pid.0,
            pgid.0,
            path,
            argc,
            envc,
            seq
        );
    }

    Some(pid)
}

pub fn set_current_exec_mappings(mappings: Vec<ExecMappedPage>) -> Option<usize> {
    let Some(current_task) = crate::task::thread::current_task() else {
        rollback_exec_mappings(&mappings);
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[execve] set current mappings failed: no current task, rolled back pages={}",
                mappings.len()
            );
        }
        return None;
    };

    let pid = {
        let task = current_task.lock();
        task.pid
    };

    let Some(process_ref) = find_process_by_pid(pid) else {
        rollback_exec_mappings(&mappings);
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[execve] set current mappings failed: missing process pid={}, rolled back pages={}",
                pid.0,
                mappings.len()
            );
        }
        return None;
    };

    let replaced = {
        let mut process = process_ref.lock();
        process.replace_exec_mappings(mappings)
    };

    let replaced_count = replaced.len();
    if replaced_count > 0 {
        rollback_exec_mappings(&replaced);
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[execve] replaced stale exec mappings: pid={} pages={}",
                pid.0,
                replaced_count
            );
        }
    }

    Some(replaced_count)
}

pub fn lookup_current_exec_mapping(virt: u64) -> Option<ExecMappedPage> {
    let current_task = crate::task::thread::current_task()?;
    let pid = {
        let task = current_task.lock();
        task.pid
    };

    let process_ref = find_process_by_pid(pid)?;
    let process = process_ref.lock();
    process
        .exec_mappings
        .iter()
        .find(|page| page.virt == virt)
        .copied()
}

fn reap_orphan_zombie_process(pid: ProcessId) {
    let can_reap = match find_process_by_pid(pid) {
        Some(process_ref) => {
            let process = process_ref.lock();
            process.parent.is_none() && process.status == ProcessStatus::Zombie
        }
        None => false,
    };

    if !can_reap {
        return;
    }

    let (removed_process, removed_tasks) = {
        let mut manager = TASK_MANAGER.lock();
        let Some(removed_process) = manager.remove_process_by_pid(pid) else {
            return;
        };
        let removed_tasks = manager.remove_tasks_by_process(pid);
        (removed_process, removed_tasks)
    };
    fs::runtime::drop_process_fds(pid.0);
    let mm_ptr = {
        let process = removed_process.lock();
        process.mm as *mut crate::mm::Mm
    };
    unsafe { crate::mm::mm_release(mm_ptr) };

    let removed_ready = SCHEDULER.remove_tasks_by_pid(pid);
    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[task] auto reap orphan process: pid={} removed_tasks={} removed_ready={}",
            pid.0,
            removed_tasks,
            removed_ready
        );
    }
}

pub fn find_task_by_tid(tid: TaskId) -> Option<Arc<Mutex<Task>>> {
    TASK_MANAGER.lock().find_task_by_tid(tid).cloned()
}

pub fn all_process_ids() -> Vec<ProcessId> {
    TASK_MANAGER.lock().all_process_ids()
}

pub fn process_ids_by_pgid(pgid: ProcessId) -> Vec<ProcessId> {
    TASK_MANAGER.lock().process_ids_by_pgid(pgid)
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
    let parent_pid = crate::task::thread::current_task().map(|task| task.lock().pid)?;
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
    let (parent_pid, current_tid) = match crate::task::thread::current_task() {
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

    let (exit_code, exec_mappings) = {
        let child_process = find_process_by_pid(child_pid)?;
        let mut child = child_process.lock();
        if child.status != ProcessStatus::Zombie {
            return None;
        }
        let exit_code = child.exit_code.unwrap_or(0);
        let exec_mappings = child.take_exec_mappings();
        (exit_code, exec_mappings)
    };

    {
        let mut parent = parent_process.lock();
        parent.remove_child(child_pid);
    }

    let removed_process = {
        let mut manager = TASK_MANAGER.lock();
        let removed_process = manager.remove_process_by_pid(child_pid)?;
        let _ = manager.remove_tasks_by_process(child_pid);
        removed_process
    };
    fs::runtime::drop_process_fds(child_pid.0);
    let mm_ptr = {
        let process = removed_process.lock();
        process.mm as *mut crate::mm::Mm
    };
    unsafe { crate::mm::mm_release(mm_ptr) };

    let removed_ready = SCHEDULER.remove_tasks_by_pid(child_pid);

    let released_mappings = exec_mappings.len();
    if released_mappings > 0 {
        rollback_exec_mappings(&exec_mappings);
    }

    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[task] reap child: parent_pid={} child_pid={} code={} removed_ready={} released_exec_pages={}",
            parent_pid.0,
            child_pid.0,
            exit_code,
            removed_ready,
            released_mappings
        );
    }

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
    spawn_kernel_thread_with_mm_mode(name, entry, SpawnMmMode::InheritShared)
}

/// 创建内核线程并添加到调度器（可失败版本，用于需要向上返回 ENOMEM 的调用方）
pub fn spawn_kernel_thread_with_mm_mode_checked(
    name: &str,
    entry: extern "C" fn(),
    mm_mode: SpawnMmMode,
) -> Option<Arc<Mutex<Task>>> {
    let (parent_pid, parent_tid) = match crate::task::thread::current_task() {
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
    let inherited_mm = resolve_child_mm(parent_pid, mm_mode)?;

    let task = Task::new_kernel_for_process(name, entry, pid, ppid);
    let task_ref = Arc::new(Mutex::new(task));

    let mut process = Process::new_kernel_with_pid(name, pid, pgid, parent_pid, parent_tid);
    process.mm = inherited_mm;
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

    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[task] spawn kernel thread: pid={} pgid={} ppid={} name={} mm_mode={:?}",
            pid.0,
            pgid.0,
            ppid.0,
            name,
            mm_mode
        );
    }

    // 添加到就绪队列
    SCHEDULER.add_task(task_ref.clone());

    Some(task_ref)
}

/// 创建内核线程并添加到调度器（可指定子进程地址空间继承策略）
pub fn spawn_kernel_thread_with_mm_mode(
    name: &str,
    entry: extern "C" fn(),
    mm_mode: SpawnMmMode,
) -> Arc<Mutex<Task>> {
    spawn_kernel_thread_with_mm_mode_checked(name, entry, mm_mode).unwrap_or_else(|| {
        panic!(
            "spawn_kernel_thread_with_mm_mode failed: mm clone OOM name={} mode={:?}",
            name, mm_mode
        )
    })
}

/// 退出当前任务
pub fn exit_current_task(exit_code: i32) {
    let Some(task_ref) = crate::task::thread::current_task() else {
        return;
    };

    let pid = {
        let mut task = task_ref.lock();
        task.status = TaskStatus::Exited;
        task.exit_code = Some(exit_code);
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[task] task exit: tid={} pid={} code={}",
                task.id.0,
                task.pid.0,
                exit_code
            );
        }
        task.pid
    };

    if let Some(process_ref) = find_process_by_pid(pid) {
        let mut should_reap_orphan = false;
        let mut released_exec_mappings: Vec<ExecMappedPage> = Vec::new();

        {
            let mut process = process_ref.lock();
            let all_exited = process
                .tasks
                .iter()
                .all(|task| task.lock().status == TaskStatus::Exited);

            if all_exited {
                process.mark_exiting(exit_code);
                process.mark_zombie();
                released_exec_mappings = process.take_exec_mappings();
                should_reap_orphan = process.parent.is_none();
                if crate::config::DEBUG_VERBOSE {
                    crate::kprintln!(
                        "\x1b[90m[diag]\x1b[0m[task] process became zombie: pid={} code={} exec_pages={}",
                        pid.0,
                        exit_code,
                        released_exec_mappings.len()
                    );
                }
            }
        }

        if !released_exec_mappings.is_empty() {
            rollback_exec_mappings(&released_exec_mappings);
        }

        if should_reap_orphan {
            reap_orphan_zombie_process(pid);
        }
    }
}

/// 退出当前进程（退出其所有任务）
pub fn exit_current_process(exit_code: i32) {
    let Some(task_ref) = crate::task::thread::current_task() else {
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

    let (released_exec_mappings, should_reap_orphan) = {
        let mut process = process_ref.lock();
        process.mark_exiting(exit_code);
        process.mark_zombie();
        (process.take_exec_mappings(), process.parent.is_none())
    };

    if !released_exec_mappings.is_empty() {
        rollback_exec_mappings(&released_exec_mappings);
    }

    // 清理同进程残留的就绪队列项，避免退出任务再次被调度。
    let removed_ready = SCHEDULER.remove_tasks_by_pid(pid);
    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[task] exit_group: pid={} code={} tasks={} removed_ready={} exec_pages={}",
            pid.0,
            exit_code,
            task_count,
            removed_ready,
            released_exec_mappings.len()
        );
    }

    if should_reap_orphan {
        reap_orphan_zombie_process(pid);
    }
}

/// 等待子进程退出（按目标回收 Zombie 子进程）
pub fn wait_child_result_by_target(target: WaitTarget) -> WaitChildResult {
    match wait_child_observe_by_target_with_options(target, WaitChildOptions::default()) {
        WaitChildObserveResult::Reapable(pid, _exit_code) => match reap_observed_child(pid) {
            Some((reaped_pid, exit_code)) => WaitChildResult::Reaped(reaped_pid, exit_code),
            None => WaitChildResult::ChildRunning,
        },
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
