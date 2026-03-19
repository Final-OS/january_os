# Process API

进程管理和进程类 syscall-facing 入口仍由 `task` 组件统一承接；ABI 边界位于 `kernel/src/task/syscall/`，运行时逻辑已按组件化宏内核规范重组到 `kernel/src/task/proc/` 与 `kernel/src/task/runtime/`。

## 当前目录边界

```text
kernel/src/task/
├── api/       # ProcessId/TaskId/TaskContext 等稳定类型
├── runtime/   # 全局任务/进程注册、查找、生命周期编排
├── proc/      # Process、fork/exec/exit/wait/signal 语义
├── thread/    # Task/Processor/KernelStack
├── sched/     # 调度器与统计
├── syscall/   # execve/clone/wait4/kill/rt_sig* ABI
└── diag/      # dump/stats 诊断入口
```

## `proc/` 主要职责

- `proc/process.rs`：`Process` 与 `ProcessStatus`
- `proc/exec.rs`：ELF 加载、地址空间切换、用户栈准备
- `proc/fork.rs`：`fork/clone/vfork` 最小语义与父子同步
- `proc/exit.rs`：当前任务/进程退出路径
- `proc/wait.rs`：`wait4` 语义、事件观测与回收
- `proc/signal.rs`：`kill/tkill/tgkill` 目标收集与状态变更
- `runtime/manager.rs`：进程表、任务表、wait 状态与 spawn 胶水

## 当前 syscall-facing 入口

```rust
pub(crate) fn sys_execve(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_getpid(_args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_getppid(_args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_gettid(_args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_clone(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_fork(_args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_vfork(_args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_getpgid(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_getpgrp(_args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_setpgid(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_setsid(_args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_kill(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_tkill(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_tgkill(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_wait4(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_exit(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_exit_group(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_rt_sigaction(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_rt_sigprocmask(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_rt_sigreturn(args: &SyscallArgs) -> SyscallRet;
```

## 当前边界

- `kernel/src/task/mod.rs`：顶层 façade、生命周期入口与稳定重导出
- `kernel/src/task/syscall/mod.rs`：仅保留 syscall 族装配与导出
- `kernel/src/task/syscall/exec_args.rs`：`execve` 参数解码与 argv/envp 边界检查
- `kernel/src/task/syscall/wait_abi.rs`：`wait4` option/status/rusage ABI 辅助
- `kernel/src/task/syscall/process.rs`：PID、进程组、clone/fork、kill、exit ABI
- `kernel/src/task/proc/*`：真实进程语义与地址空间协作
- `kernel/src/task/runtime/manager.rs`：运行态全局表与 wait/spawn 共享路径

## 当前 `execve` 状态

- `sys_execve()` 已通过 `fs::runtime::read_all_for_pid()` 从 VFS 路径读取镜像，不再依赖内置静态镜像表
- `execve` 可直接执行手工挂载后的 FAT32/ext4 路径；`ksh exec` 对相对路径会先按 shell cwd 归一化
- 初始用户栈已构建 `argc/argv/envp/auxv` 最小布局，并维护 `mm.arg_*` / `mm.env_*` 区间
- 当前 auxv 仅提供 `AT_PAGESZ/AT_PHDR/AT_PHENT/AT_PHNUM/AT_ENTRY`
- 仍缺 `PT_INTERP`、动态链接器、TLS、shebang 与更完整 ABI 兼容性

## 相关文档

- [Task API](./task.md)
- [Scheduler API](./scheduler.md)
- [Syscall API](../syscall/syscall.md)
