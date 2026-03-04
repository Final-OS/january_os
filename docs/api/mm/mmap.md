# mmap API

本文档描述用户态内存映射相关接口的当前实现与约束。

## 目标能力

- `mmap`：建立用户虚拟地址到物理页/文件页的映射
- `munmap`：解除映射并回收资源
- `mprotect`：调整已有映射区域访问权限
- `brk`：管理进程堆区边界

## 关键接口（当前）

```rust
pub fn sys_mmap(args: &SyscallArgs) -> SyscallRet;
pub fn sys_munmap(args: &SyscallArgs) -> SyscallRet;
pub fn sys_mprotect(args: &SyscallArgs) -> SyscallRet;
pub fn sys_brk(args: &SyscallArgs) -> SyscallRet;
pub fn do_mmap(mm: &mut MmStruct, req: &MmapRequest) -> Result<u64, MmError>;
pub fn do_munmap(mm: &mut MmStruct, addr: u64, len: u64) -> Result<(), MmError>;
```

## 当前实现约束

- `mmap`:
  - 仅接受 `MAP_SHARED/MAP_PRIVATE` 二选一；非法组合返回 `-EINVAL`。
  - 支持匿名映射与 FD 映射的最小路径；`offset` 需按页对齐。
  - `MAP_FIXED` 命中已有映射时会先执行重叠区域拆分与回收。
- `munmap`:
  - 要求起始地址页对齐；长度会向上页对齐后处理。
  - 仅处理用户空间区间，越界返回错误。
- `mprotect`:
  - 对指定区间执行 VMA 切分、权限更新与页表标志同步。
- `brk`:
  - 支持查询当前 break（`arg0=0`）和按策略扩展/收缩堆边界。

## 相关文档

- [VMA API](./vma.md)
- [Paging API](./paging.md)
- [Fault API](./fault.md)
- [Syscall API](../syscall/syscall.md)
