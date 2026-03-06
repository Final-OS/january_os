# File API

文件与文件描述符相关 syscall-facing 入口现在归 `FS` 组件所有；组件已按 `façade + 分层子目录骨架` 重组，顶层 façade 位于 `kernel/src/fs/mod.rs`，运行时与 VFS 能力分别落在 `runtime/fd/pipe/backing/vfs/syscall` 子域。

## 当前目录边界

```text
kernel/src/fs/
├── mod.rs        # façade：生命周期、稳定导出、跨域胶水
├── api/          # Metadata、DirEntry、FsError、SeekWhence
├── runtime/      # FS_STATE、stdin wait、初始化与状态管理
├── fd/           # File trait、目录游标占位、FD 运行时边界
├── vfs/          # inode/path/mount/filesystem/lookup 核心
├── pipe/         # pipe/poll/select 运行时边界
├── backing/      # initramfs 与 mmap 文件后备
├── syscall/      # open/read/write/... 的 ABI 入口
└── diag/         # dump/stats
```

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

- `kernel/src/fs/runtime/manager.rs`：FD 表、cwd、pipe 状态、mmap 文件后备与初始化主路径
- `kernel/src/fs/runtime/stdin.rs`：stdin 等待队列与唤醒兼容入口
- `kernel/src/fs/fd/`：打开文件/目录运行时边界与 `File` trait
- `kernel/src/fs/vfs/`：路径解析、inode、mount、filesystem 抽象
- `kernel/src/fs/backing/initramfs.rs`：initramfs 后端
- `kernel/src/fs/backing/mmap.rs`：mmap 文件后备引用计数与页复制接口
- `kernel/src/fs/syscall/`：仅保留 ABI 解码和用户态访问辅助

## 当前实现状态

- `read/write/close/stat/fstat/lstat/poll/pipe/select/pipe2` 已具备运行路径
- `open/ioctl/lseek/dup/dup2/fcntl/chdir/getcwd/getdents64` 已有实现，但仍属于最小/受限语义
- `fs::runtime::*` 与 `fs::backing::*` 兼容入口仍保留，供 `task/mm/shell/tests` 调用

## 相关文档

- [Syscall API](../syscall/syscall.md)
