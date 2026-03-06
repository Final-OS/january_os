# mmap API

用户态内存映射相关 syscall-facing 入口现在归 `MM` 组件所有，实现在 `kernel/src/mm/syscall/`。

## 当前入口

```rust
pub(crate) fn sys_mmap(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_munmap(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_mprotect(args: &SyscallArgs) -> SyscallRet;
pub(crate) fn sys_brk(args: &SyscallArgs) -> SyscallRet;
```

## 组件职责

- `kernel/src/uaccess.rs`：跨组件共享的用户态地址范围校验、结构体/整数/C 字符串读写基础设施
- `kernel/src/mm/syscall/txn.rs`：VMA/PTE 事务、回滚与 backing 调整
- `kernel/src/mm/syscall/addr.rs`：mmap 地址选择、FD 解析和 unmap 区间收集
- `kernel/src/mm/syscall/mmap.rs`：`mmap` / `munmap` ABI 入口
- `kernel/src/mm/syscall/protect.rs`：`brk` / `mprotect` ABI 入口

- `sys_mmap`：解析 Linux ABI 参数，建立匿名映射或文件后备映射
- `sys_munmap`：执行区间拆分、取消映射与页回收
- `sys_mprotect`：执行 VMA 切分、权限更新与页表标志同步
- `sys_brk`：管理当前进程堆区边界
- `kernel/src/mm/syscall/`：仅承载 `mmap/munmap/mprotect/brk` ABI 入口，不再向其他组件暴露共享 `uaccess`

## 当前约束

- `mmap` 仅接受 `MAP_SHARED/MAP_PRIVATE` 二选一
- `MAP_LOCKED` / `MAP_HUGETLB` 当前返回 `-EINVAL`
- `mprotect` 仍是最小可用实现，语义覆盖未完全补齐
- `brk` 支持查询与基础扩缩容策略

## 相关文档

- [VMA API](./vma.md)
- [Paging API](./paging.md)
- [Fault API](./fault.md)
- [Syscall API](../syscall/syscall.md)
