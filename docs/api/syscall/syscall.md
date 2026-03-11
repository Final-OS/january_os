# Syscall API

`syscall` 子系统现在只保留 ABI 壳层，不再承载文件系统、内存管理、进程管理的真实业务实现。

## 当前组织

- `kernel/src/syscall/`：统一参数结构、返回值编码、号表和跨架构分发接口
- `kernel/src/syscall/arch/x86_64/`：x86_64 Linux ABI 编号表与按号精确分发的入口绑定
- `kernel/src/arch/x86_64/syscall/`：`syscall` 指令陷入入口、寄存器保存与返回路径
- `kernel/src/fs/syscall/`：FS 域 ABI 入口；真实运行时分布在 `fs/runtime/fd/pipe/backing/vfs`
- `kernel/src/mm/syscall/`：`mmap/munmap/mprotect/brk` 专属 ABI 入口
- `kernel/src/task/syscall/`：`execve/fork/clone/wait4/kill/rt_sig*` 等 task 域 ABI 入口
- `kernel/src/task/proc/` + `kernel/src/task/runtime/`：进程语义、全局表、wait/spawn 运行时

## ABI 约定

x86_64 Long Mode 继续使用 Linux `syscall` 寄存器约定：

- `rax`：系统调用号 / 返回值
- `rdi`：参数 0
- `rsi`：参数 1
- `rdx`：参数 2
- `r10`：参数 3
- `r8`：参数 4
- `r9`：参数 5

返回值仍使用 Linux errno 编码：成功返回非负值，失败返回 `-errno`。

## 核心接口

```rust
pub struct SyscallArgs {
    pub nr: usize,
    pub arg0: usize,
    pub arg1: usize,
    pub arg2: usize,
    pub arg3: usize,
    pub arg4: usize,
    pub arg5: usize,
}

pub struct SyscallDef {
    pub nr: usize,
    pub name: &'static str,
    pub domain: SyscallDomain,
}

pub trait SyscallArch {
    fn dispatch(&self, args: &SyscallArgs) -> SyscallRet;
    fn syscall_table(&self) -> &'static [SyscallDef];
}

pub fn dispatch(
    nr: usize,
    arg0: usize,
    arg1: usize,
    arg2: usize,
    arg3: usize,
    arg4: usize,
    arg5: usize,
) -> SyscallRet;

pub fn syscall_table() -> &'static [SyscallDef];
```

补充说明：

- `x86_64` 当前导出的 `syscall_table()` 按 Linux ABI 号位连续覆盖 `0..=461`
- 其中 `335..=423` 目前是显式 `reserved` 占位项，用于保持号位视图连续；这些号位并不表示 january_os 已实现对应 syscall

## 组件分工

### `fs::syscall`

负责：`open/stat/lstat/fstat/read/write/close/lseek/dup/dup2/fcntl/chdir/getcwd/getdents64/pipe/pipe2/ioctl/poll/select`。

当前进一步拆为：
- `kernel/src/fs/syscall/file.rs`：路径、FD、目录项与普通读写 ABI
- `kernel/src/fs/syscall/pipe.rs`：pipe、pipe2、ioctl ABI
- `kernel/src/fs/syscall/poll.rs`：poll、select ABI

### `mm::syscall`

负责：`mmap/munmap/mprotect/brk`。共享用户缓冲区/用户结构体/用户 C 字符串读写校验统一由 `kernel/src/uaccess.rs` 提供。

### `task::syscall`

负责：`execve/getpid/getppid/gettid/clone/fork/vfork/getpgid/getpgrp/setpgid/setsid/kill/tkill/tgkill/wait4/exit/exit_group/rt_sig*`。

当前进一步拆为：
- `kernel/src/task/syscall/exec.rs`：`execve` ABI 入口
- `kernel/src/task/syscall/process.rs`：PID、进程组、clone/fork、kill、exit
- `kernel/src/task/proc/`：真实进程语义与地址空间/调度协作
- `kernel/src/task/syscall/wait.rs`：`wait4` ABI 解码与结果写回
- `kernel/src/task/syscall/signal.rs`：`rt_sig*` ABI 入口

## 当前边界原则

- `syscall` 只做 ABI 解码与按 Linux ABI 号精确分发
- 不使用自定义号段推导组件归属；domain 仅作为诊断/统计元数据
- 真实内核语义落在 `fs/mm/task/net/security/virt` 各自组件内
- 架构相关实现只放在 `arch/<arch>` 路径下

## 相关文档

- [FS File API](../fs/file.md)
- [MM mmap API](../mm/mmap.md)
- [Task Process API](../task/process.md)
