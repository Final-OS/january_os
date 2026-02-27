# Processor API

Processor 模块维护当前 CPU 的任务上下文与运行状态。

## 主要职责

- 追踪当前 CPU 正在运行的任务
- 为调度器提供 per-CPU 运行状态
- 在上下文切换前后更新 CPU 本地信息

## 关键接口

```rust
pub fn current_task() -> Option<Arc<Mutex<Task>>>;
pub fn set_current_task(task: Option<Arc<Mutex<Task>>>);
pub fn cpu_id() -> usize;
```

## 相关文档

- [Task API](./task.md)
- [Scheduler API](./scheduler.md)
