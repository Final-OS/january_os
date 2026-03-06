# File API

文件与文件描述符相关 syscall-facing 入口现在归 `FS` 组件所有，主要实现位于 `kernel/src/fs/syscall/`，底层状态与 VFS 能力位于 `kernel/src/fs/mod.rs` 和 `kernel/src/fs/vfs/`。

## 当前入口

```rust
pub(crate) fn sys_open(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_stat(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_lstat(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_fstat(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_read(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_write(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_close(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_lseek(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_dup(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_dup2(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_fcntl(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_chdir(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_getcwd(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_getdents64(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_pipe(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_pipe2(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_ioctl(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_poll(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_select(args: &SyscallArgs) -> SyscallRet;
```

## 组件职责

- `kernel/src/fs/syscall/mod.rs`：共享 Linux ABI 类型、常量、等待队列与通用辅助
- `kernel/src/uaccess.rs`：跨组件共享的用户态访问基础设施；FS 仅在 `fs/syscall/uaccess.rs` 保留本组件专属 ABI 辅助
- `kernel/src/fs/syscall/stdin.rs`：stdin 等待队列、TTY 输入读取与唤醒
- `kernel/src/fs/syscall/file.rs`：路径、FD、目录项与普通读写入口
- `kernel/src/fs/syscall/pipe.rs`：pipe、pipe2、ioctl 入口
- `kernel/src/fs/syscall/poll.rs`：poll/select 入口
- FD 表、cwd、pipe 状态和 mmap 文件后备都归 `FS_STATE`
- TTY、键盘、串口输入等待队列归 `FS` 组件管理
- `syscall` 顶层不再保存文件语义实现

## 当前实现状态

- `read/write/close/stat/fstat/lstat/poll/pipe/select/pipe2` 已具备运行路径
- `open/ioctl/lseek/dup/dup2/fcntl/chdir/getcwd/getdents64` 已有实现，但仍属于最小/受限语义

## 相关文档

- [Syscall API](../syscall/syscall.md)
