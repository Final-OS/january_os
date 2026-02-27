# Scheduler API

调度器负责选择下一个可运行任务并触发上下文切换。

## 主要职责

- 维护就绪队列
- 处理任务入队/出队
- 在时钟中断或主动让出时执行调度

## 关键接口

```rust
pub fn schedule();
pub fn add_task(task: Arc<Mutex<Task>>);
pub fn remove_tasks_by_pid(pid: ProcessId) -> usize;
```

## 相关文档

- [Task API](./task.md)
- [Processor API](./processor.md)
- [Context API](./context.md)
