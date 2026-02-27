# Context API

Context 模块定义任务切换时保存/恢复的寄存器上下文。

## 主要职责

- 定义 `TaskContext` 布局
- 配合汇编入口完成 `__switch`
- 构建新任务初始上下文

## 关键接口

```rust
pub struct TaskContext;
pub unsafe fn __switch(prev: *mut TaskContext, next: *const TaskContext);
```

## 相关文档

- [Task API](./task.md)
- [Scheduler API](./scheduler.md)
