# Process API

进程管理和进程类 syscall-facing 入口由 `task` 组件统一承接，ABI 边界位于 `kernel/src/task/syscall/`，核心运行时仍分别位于 `kernel/src/task/process/*`。

## 主要职责

- 维护 PID、父子关系、进程组、会话与退出状态
- 为 `wait4`、`kill/tkill/tgkill`、`setpgid/setsid` 提供状态基础
- 为 `execve/fork/clone/vfork` 提供运行时与地址空间协作
- 为 `rt_sigaction/rt_sigprocmask/rt_sigreturn` 提供最小信号 ABI 入口

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

- `kernel/src/task/syscall/mod.rs`：仅保留 syscall 族装配与模块导出
- `kernel/src/task/syscall/exec_args.rs`：`execve` 参数解码；基础用户态读指针/整数范围校验复用 `kernel/src/uaccess.rs`，并保留 argv/envp 空串语义差异
- `kernel/src/task/syscall/wait_abi.rs`：`wait4` option/status/rusage ABI 辅助；用户态写回复用 `kernel/src/uaccess.rs`
- `kernel/src/task/syscall/exec.rs`：`execve` ABI 解码
- `kernel/src/task/syscall/process.rs`：PID、clone/fork、进程组、kill、exit
- `kernel/src/task/syscall/wait.rs`：`wait4` ABI 解码与状态/rusage 写回
- `kernel/src/task/syscall/signal.rs`：`rt_sig*` ABI 入口；用户态 sigaction/sigset 读写复用 `kernel/src/uaccess.rs`
- `task/process/exec.rs`、`fork.rs`、`signal.rs`、`wait.rs` 继续承载真实运行时逻辑
- `syscall` 顶层不再保存进程/信号业务实现

## 相关文档

- [Task API](./task.md)
- [Syscall API](../syscall/syscall.md)
