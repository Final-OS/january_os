# Scheduler API

调度器负责选择下一个可运行任务并触发上下文切换，当前实现位于 `kernel/src/task/sched/mod.rs`。

## 主要职责

- 维护 per-CPU 就绪队列
- 处理本地挑选与简单 work-stealing
- 在主动让出或时钟推进时执行调度
- 汇总 local/steal/idle 调度统计

## 关键接口

```rust
pub fn schedule();
pub fn run_idle() -> !;
pub fn snapshot_stats() -> SchedulerStats;
pub fn add_task(task: Arc<Mutex<Task>>);
pub fn remove_tasks_by_pid(pid: ProcessId) -> usize;
```

## 当前边界

- `task::sched`：统一调度入口与统计
- `task::thread::processor`：当前 CPU task 交换
- `task::runtime::manager`：按 PID 删除与进程态协作

## 相关文档

- [Task API](./task.md)
- [Processor API](./processor.md)
- [Context API](./context.md)
