# Process API

Process 模块管理进程级状态（PID、父子关系、进程组、退出状态）。

## 主要职责

- 维护进程生命周期
- 维护父子关系与回收状态（Zombie/Reap）
- 为 `wait4`、`kill`、`setpgid/setsid` 提供状态基础

## 关键接口

```rust
pub fn find_process_by_pid(pid: ProcessId) -> Option<Arc<Mutex<Process>>>;
pub fn current_pid() -> Option<ProcessId>;
pub fn wait_child_result_by_target(target: WaitTarget) -> WaitChildResult;
```

## 相关文档

- [Task API](./task.md)
- [Syscall API](../syscall/syscall.md)
