# Syscall (System Call) API

系统调用子系统提供用户态程序与内核交互的接口，实现 Linux ABI 兼容。

---

## 系统调用机制

### syscall 指令入口

x86_64 Long Mode 使用 `syscall` 指令进入内核态，当前实现通过 `iretq` 返回用户态。

**寄存器约定**:
- `rax`: 系统调用号（输入）/ 返回值（输出）
- `rdi`: 参数 0
- `rsi`: 参数 1
- `rdx`: 参数 2
- `r10`: 参数 3
- `r8`: 参数 4
- `r9`: 参数 5

**当前实现要点**:
- BSP/AP 初始化阶段会写入 `STAR/LSTAR/SFMASK/EFER.SCE`。
- `syscall_entry` 汇编入口会把寄存器参数组装后转发到统一 syscall 分发。
- 返回路径当前使用 `iretq`（后续可继续收敛到 `sysretq`）。

---

## 核心结构

### SyscallArgs

系统调用参数结构。

```rust
pub struct SyscallArgs {
    pub nr: usize,      // 系统调用号
    pub arg0: usize,    // 参数 0
    pub arg1: usize,    // 参数 1
    pub arg2: usize,    // 参数 2
    pub arg3: usize,    // 参数 3
    pub arg4: usize,    // 参数 4
    pub arg5: usize,    // 参数 5
}
```

### SyscallDef

系统调用定义。

```rust
pub struct SyscallDef {
    pub nr: usize,           // 系统调用号
    pub name: &'static str,  // 系统调用名称
}
```

### SyscallHandler

系统调用处理函数类型。

```rust
pub type SyscallHandler = fn(&SyscallArgs) -> SyscallRet;
pub type SyscallRet = usize;
```

---

## 系统调用分发

### dispatch

分发系统调用到对应的处理函数。

```rust
pub fn dispatch(
    nr: usize,
    arg0: usize,
    arg1: usize,
    arg2: usize,
    arg3: usize,
    arg4: usize,
    arg5: usize,
) -> SyscallRet
```

**参数**:
- `nr`: 系统调用号
- `arg0-arg5`: 系统调用参数

**返回**: `SyscallRet` - 返回值或错误码

---

## 错误码

### 标准错误码

```rust
pub const EBADF: i32 = 9;      // Bad file descriptor
pub const ECHILD: i32 = 10;    // No child processes
pub const ENOMEM: i32 = 12;    // Out of memory
pub const EFAULT: i32 = 14;    // Bad address
pub const EBUSY: i32 = 16;     // Resource busy
pub const EINVAL: i32 = 22;    // Invalid argument
pub const ENOSYS: i32 = 38;    // Function not implemented
```

### 错误处理

```rust
// 返回成功
pub(crate) fn ok(ret: usize) -> usize {
    ret
}

// 返回错误
pub(crate) fn err(errno: i32) -> usize {
    (-(errno as isize)) as usize
}
```

---

## 已实现的系统调用

### 进程管理

#### sys_getpid (39)

获取当前进程 ID。

```rust
pub(crate) fn sys_getpid(_args: &SyscallArgs) -> SyscallRet
```

**返回**: 进程 ID 或错误码

**示例**:
```c
// 用户态 C 代码
pid_t pid = getpid();
```

#### sys_getppid (110)

获取父进程 ID。

```rust
pub(crate) fn sys_getppid(_args: &SyscallArgs) -> SyscallRet
```

**返回**: 父进程 ID 或 0

#### sys_gettid (186)

获取线程 ID。

```rust
pub(crate) fn sys_gettid(_args: &SyscallArgs) -> SyscallRet
```

**返回**: 线程 ID 或错误码

#### sys_clone (56)

创建子进程（最小 Linux ABI 语义）。

```rust
pub(crate) fn sys_clone(args: &SyscallArgs) -> SyscallRet
```

**当前实现约束**:
- 支持 `CSIGNAL` 与 `CLONE_VFORK|CLONE_VM` 的最小子集。
- 暂不支持 `CLONE_THREAD/CLONE_SETTLS/CLONE_*TID` 等线程共享语义；命中后返回 `-EINVAL`。
- 暂不支持 `child_stack/ptid/ctid/tls` 非零参数；命中后返回 `-EINVAL`。
- 目前子进程以最小内核线程形态创建，用于打通创建/退出/回收链路。

#### sys_fork (57)

`fork` 的最小可用语义，当前内部基于 `clone(SIGCHLD)` 路径。

```rust
pub(crate) fn sys_fork(_args: &SyscallArgs) -> SyscallRet
```

#### sys_vfork (58)

`vfork` 的最小可用语义，当前内部基于 `clone(CLONE_VFORK|CLONE_VM|SIGCHLD)`，父进程会等待子进程释放执行权。

```rust
pub(crate) fn sys_vfork(_args: &SyscallArgs) -> SyscallRet
```

#### sys_execve (59)

执行新程序映像（Batch 3 第三阶段）。

```rust
pub(crate) fn sys_execve(args: &SyscallArgs) -> SyscallRet
```

**当前行为**:
- 已接入 `pathname/argv/envp` 的用户指针范围与字符串边界校验。
- 支持 `EFAULT/E2BIG/ENAMETOOLONG/ENOENT` 等基础错误路径。
- 已接入最小 ELF64 解析、`PT_LOAD` 规划与权限检查（含段重叠校验）。
- 已接入 `PT_LOAD + user stack` 真实映射（按段 PTE 权限），并在失败路径执行回滚。
- 映射目标页若已存在映射，返回 `-EBUSY`（避免覆盖现有地址空间）。
- 已接入用户态 `iretq` 入口帧构建骨架（`rip/rsp/cs/ss/rflags`）。
- 当前内置可解析镜像路径：`/init`、`/bin/demo_user`。
- 会记录本次 exec 请求到进程可观测状态（path/argc/envc/seq）。
- 当前仍返回 `-ENOSYS`：ring3 实际切换尚未并入 `sys_execve` 主路径，映射在本阶段会回滚。
- 可通过 shell `runuser` 命令验证 ring3 + syscall 最小链路（演示路径）。
- `runuser` 演示链路已可稳定完成 `ring3 -> sys_exit(60) -> 任务回收 -> 返回 shell`。
- 映射阶段已补充页级诊断日志（含冲突检测、映射/回滚、syscall 入口与返回日志）。

#### sys_getpgid (121) / sys_getpgrp (111)

获取进程组 ID。

```rust
pub(crate) fn sys_getpgid(args: &SyscallArgs) -> SyscallRet
pub(crate) fn sys_getpgrp(_args: &SyscallArgs) -> SyscallRet
```

#### sys_setpgid (109) / sys_setsid (112)

设置进程组和会话（最小语义）。

```rust
pub(crate) fn sys_setpgid(args: &SyscallArgs) -> SyscallRet
pub(crate) fn sys_setsid(_args: &SyscallArgs) -> SyscallRet
```

#### sys_kill (62) / sys_tkill (200) / sys_tgkill (234)

发送信号（当前支持 `0/SIGCHLD/SIGTERM/SIGKILL/SIGSTOP/SIGCONT`）。

```rust
pub(crate) fn sys_kill(args: &SyscallArgs) -> SyscallRet
pub(crate) fn sys_tkill(args: &SyscallArgs) -> SyscallRet
pub(crate) fn sys_tgkill(args: &SyscallArgs) -> SyscallRet
```

**注意**:
- `SIGSTOP/SIGCONT` 会驱动进程停止/继续事件，并可由 `wait4 + WUNTRACED/WCONTINUED` 观测。
- `SIGTERM/SIGKILL` 走进程退出路径并进入 `Zombie`，随后可由 `wait4` 回收。

#### sys_exit (60)

退出当前任务。

```rust
pub(crate) fn sys_exit(args: &SyscallArgs) -> SyscallRet
```

**参数**:
- `arg0`: 退出码

**示例**:
```c
// 用户态 C 代码
exit(0);
```

#### sys_exit_group (231)

退出当前进程组。

```rust
pub(crate) fn sys_exit_group(args: &SyscallArgs) -> SyscallRet
```

**参数**:
- `arg0`: 退出码

#### sys_wait4 (61)

等待子进程退出（增强最小实现）。

```rust
pub(crate) fn sys_wait4(args: &SyscallArgs) -> SyscallRet
```

**参数**:
- `arg0`: 进程过滤
  - `> 0`: 等待指定 PID
  - `-1`: 等待任意子进程
  - `0`: 等待“同进程组”子进程
  - `< -1`: 等待指定进程组（`abs(arg0)`）
- `arg1`: 状态指针
- `arg2`: 选项
- `arg3`: rusage 指针

**返回**: 子进程 PID 或错误码

**注意**:
- 若 `arg1 != 0`，内核会写入 wait status：
  - 正常退出：`(exit_code & 0xff) << 8`
  - 停止事件（`WUNTRACED`）：`((signal & 0xff) << 8) | 0x7f`
  - 继续事件（`WCONTINUED`）：`0xffff`
- 若 `arg3 != 0`，内核会写入 `rusage`（当前基于任务运行时 tick + 上下文切换计数聚合）。
- 支持 `WNOHANG`（`arg2 & 0x1`）；若有匹配子进程但尚无可报告事件，返回 `0`。
- 支持 `WUNTRACED/WCONTINUED`：可分别报告子进程停止/继续事件。
- 支持 `__WCLONE/__WALL`：按 Linux 语义切换 clone/non-clone 子进程匹配范围。
- 支持 `__WNOTHREAD`：仅等待“当前线程创建的子进程”；未设置时可等待同进程内其他线程创建的子进程。
- 若不存在匹配子进程，返回 `-ECHILD`。
- 若未设置 `WNOHANG` 且子进程尚未退出，当前实现采用调度循环等待（协作式阻塞）。
- 非上述已知选项位返回 `-EINVAL`。

### I/O 操作

#### sys_write (1)

写入数据（桩实现）。

```rust
pub(crate) fn sys_write(args: &SyscallArgs) -> SyscallRet
```

**参数**:
- `arg0`: 文件描述符
- `arg1`: 缓冲区指针
- `arg2`: 字节数

**返回**: 写入的字节数或错误码

**注意**: 当前是桩实现，只支持写入到控制台。

---

## 系统调用表

### Linux ABI 系统调用

完整的 Linux x86_64 系统调用表（300+ 系统调用）。

**部分系统调用列表**:

| 号码 | 名称 | 状态 |
|------|------|------|
| 0 | read | ❌ 未实现 |
| 1 | write | ⚠️ 桩实现 |
| 2 | open | ❌ 未实现 |
| 3 | close | ❌ 未实现 |
| 9 | mmap | ❌ 未实现 |
| 39 | getpid | ✅ 已实现 |
| 56 | clone | ⚠️ 最小实现（受限 flags） |
| 57 | fork | ⚠️ 最小实现 |
| 58 | vfork | ⚠️ 最小实现 |
| 59 | execve | ⚠️ 参数校验+真实映射回滚（返回 -ENOSYS） |
| 60 | exit | ✅ 已实现 |
| 61 | wait4 | ✅ 增强实现 |
| 62 | kill | ⚠️ 子集实现 |
| 109 | setpgid | ⚠️ 最小实现 |
| 110 | getppid | ✅ 已实现 |
| 111 | getpgrp | ✅ 已实现 |
| 112 | setsid | ⚠️ 最小实现 |
| 121 | getpgid | ✅ 已实现 |
| 186 | gettid | ✅ 已实现 |
| 200 | tkill | ⚠️ 子集实现 |
| 231 | exit_group | ✅ 已实现 |
| 234 | tgkill | ⚠️ 子集实现 |

### 获取系统调用表

```rust
pub fn syscall_table() -> &'static [SyscallDef]
```

**返回**: 系统调用定义数组

---

## 使用示例

### 内核态调用系统调用

```rust
use crate::syscall;

// 调用 getpid
let pid = syscall::dispatch(39, 0, 0, 0, 0, 0, 0);
println!("PID: {}", pid);

// 调用 exit
syscall::dispatch(60, 0, 0, 0, 0, 0, 0);
```

### 用户态调用系统调用（未来）

```c
// C 代码示例
#include <unistd.h>
#include <sys/types.h>

int main() {
    pid_t pid = getpid();
    printf("My PID: %d\n", pid);

    pid_t ppid = getppid();
    printf("Parent PID: %d\n", ppid);

    exit(0);
}
```

### 汇编调用系统调用

```asm
; x86_64 汇编
mov rax, 39        ; sys_getpid
syscall            ; 执行系统调用
; rax 现在包含 PID
```

---

## 实现新的系统调用

### 步骤

1. **定义处理函数**

```rust
// kernel/src/syscall/handlers/process.rs
pub(crate) fn sys_my_syscall(args: &SyscallArgs) -> SyscallRet {
    let arg0 = args.arg0;
    let arg1 = args.arg1;

    // 实现系统调用逻辑
    // ...

    ok(result)
}
```

2. **添加到系统调用表**

```rust
// kernel/src/syscall/arch/x86_64/table.rs
pub const SYSCALL_TABLE: &[SyscallDef] = &[
    // ...
    SyscallDef { nr: 999, name: "my_syscall" },
];
```

3. **添加到分发器**

```rust
// kernel/src/syscall/arch/x86_64/mod.rs
impl SyscallArch for X86_64Syscall {
    fn dispatch(&self, args: &SyscallArgs) -> SyscallRet {
        match args.nr {
            // ...
            999 => handlers::process::sys_my_syscall(args),
            _ => err(ENOSYS),
        }
    }
}
```

---

## 参数验证

### 用户指针验证

```rust
// 验证用户态指针是否有效
fn validate_user_ptr<T>(ptr: *const T) -> Result<(), i32> {
    if ptr.is_null() {
        return Err(EFAULT);
    }

    // 检查地址是否在用户空间
    let addr = ptr as usize;
    if addr >= KERNEL_BASE {
        return Err(EFAULT);
    }

    Ok(())
}
```

### 字符串验证

```rust
// 从用户态读取字符串
fn read_user_string(ptr: *const u8, max_len: usize) -> Result<String, i32> {
    validate_user_ptr(ptr)?;

    // 读取字符串
    // ...

    Ok(string)
}
```

---

## 性能考虑

### 系统调用开销

- **syscall/sysret**: ~100 cycles
- **参数传递**: 寄存器传递，无额外开销
- **上下文切换**: 保存/恢复寄存器

### 优化建议

1. **批量操作**: 使用批量系统调用减少调用次数
2. **缓存**: 缓存频繁访问的数据
3. **vDSO**: 将简单系统调用映射到用户空间

---

## 安全性

### 权限检查

```rust
// 检查当前进程是否有权限
fn check_permission(required: Permission) -> Result<(), i32> {
    let current = current_task().ok_or(EINVAL)?;
    let task = current.lock();

    if !task.has_permission(required) {
        return Err(EPERM);
    }

    Ok(())
}
```

### 参数验证

- 验证所有用户态指针
- 检查参数范围
- 防止整数溢出
- 防止路径遍历

---

## 限制和注意事项

1. **当前限制**
   - 只支持内核态调用
   - 大部分系统调用未实现
   - 缺少参数验证
   - 缺少权限检查

2. **未来改进**
   - 实现用户态支持
   - 完善系统调用实现
   - 添加参数验证
   - 实现权限系统

3. **调试建议**
   - 使用 `strace` 跟踪系统调用（未来）
   - 记录系统调用日志
   - 检查返回值

---

## 相关 API

- [Task API](../task/task.md) - 任务管理
- [Process API](../task/process.md) - 进程管理
- [File API](../fs/file.md) - 文件操作（未来）
- [Memory API](../mm/mmap.md) - 内存映射（未来）

---

**最后更新**: 2026-02-08
