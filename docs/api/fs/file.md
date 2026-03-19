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
├── backing/      # initramfs / FAT32 / ext4 / mmap 文件后备
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
- `kernel/src/fs/backing/fat/`：FAT32 只读后端（已支持短文件名 + LFN 读取，仍无写路径）
- `kernel/src/fs/backing/ext4/`：ext4 只读后端（当前支持 4KiB block、extent tree 读取与基础目录/htree-root 兼容）
- `kernel/src/fs/backing/mmap.rs`：mmap 文件后备引用计数与页复制接口
- `kernel/src/fs/syscall/`：仅保留 ABI 解码和用户态访问辅助

## 当前实现状态

- `read/write/close/stat/fstat/lstat/poll/pipe/select/pipe2` 已具备运行路径
- `open/ioctl/lseek/dup/dup2/fcntl/chdir/getcwd/getdents64` 已有实现，但仍属于最小/受限语义
- 启动阶段会保留 `initramfs` 作为 `/`；`/mnt` 只是 rootfs 内的普通目录，默认放置 `fat32.img` / `ext4.img` 样例镜像与空挂载点目录
- 启动阶段仍会扫描 `virtio-blk` 分区生成可挂载 source 列表，但不会自动挂载
- `ksh` 现在提供最小 `mount/umount/remount` 生命周期：`mount` 可列出已挂载文件系统与可挂载 source，`mount -t <fat32|ext4> <source> <target>` 可手工挂载分区 source 或镜像文件路径（如 `/mnt/fat32.img`）；`exec <path>` 则支持按 shell cwd 解析相对路径
- 默认 `make run` / `make debug` 会生成带样例文件的 FAT32 分区数据盘，同时把 `fat32.img` / `ext4.img` 复制进 initramfs `/mnt`；镜像中预置 `HELLO.ELF`、`HELLO.TXT`、`LONG-FILE.TXT`、`README.TXT`，其中 `HELLO.ELF` 来自 `userland/hello`，执行后只打印一行 `HELLO` 并退出
- `fs::runtime::read_all_for_pid()` 已走 VFS 路径，`execve`/bootstrap 可直接从挂载文件系统读取 ELF
- `fs::runtime::*` 与 `fs::backing::*` 兼容入口仍保留，供 `task/mm/shell/tests` 调用

## v0.3.0 严格边界

- `v0.3.0` 已完成的是最小主链路：`virtio-blk` 分区探测、只读 FAT32/ext4 挂载、VFS 路径解析、最小文件 syscall、以及从挂载文件系统读取 ELF
- `v0.3.0` 未完成的是更完整生命周期与 ABI：通用 `mount(2)` / `umount(2)` syscall、用户态 `mount(8)`、可写 loop、完整挂载标志、`statfs/fstatfs/dup3`、更完整 `auxv`
- 当前 `/` 仍固定为 `initramfs`；磁盘文件系统不是默认 rootfs，而是人工验证路径

## 推荐验证方式

- `make build`
- `make run`
- 在 shell 中执行 `mount`，确认可挂载 source 与 `/mnt/fat32.img`、`/mnt/ext4.img`
- `mount -t fat32 <source-or-image> <target>` 后验证 `ls`、`cat`、`exec`
- `mount -t ext4 <source-or-image> <target>` 后验证文件读取和 `exec`
- 运行 `test vfs`、`test fs fat32`、`test fs ext4`

## 相关文档

- [Syscall API](../syscall/syscall.md)
