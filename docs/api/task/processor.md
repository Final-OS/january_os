# Processor API

Processor 模块维护当前 CPU 的任务上下文与运行状态，当前实现位于 `kernel/src/task/thread/processor.rs`。

## 主要职责

- 追踪当前 CPU 正在运行的任务
- 为调度器提供 per-CPU current task 状态
- 在上下文切换前后包装 `__switch` 调用

## 关键接口

```rust
pub fn current_task() -> Option<Arc<Mutex<Task>>>;
pub(crate) fn replace_current_task(next: Arc<Mutex<Task>>) -> Option<Arc<Mutex<Task>>>;
pub(crate) fn take_current_task() -> Option<Arc<Mutex<Task>>>;
pub(crate) unsafe fn do_switch(prev_ctx_ptr: *mut usize, next_ctx_ptr: *const usize);
```

## 当前边界

- `thread/task.rs`：`Task`/`TaskStatus`/`KernelStack`
- `thread/processor.rs`：per-CPU 当前任务状态与切换包装
- `arch/<arch>/`：架构专属上下文布局与底层切换实现

## 相关文档

- [Task API](./task.md)
- [Scheduler API](./scheduler.md)
