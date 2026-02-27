# mmap API

本文档描述用户态内存映射相关接口规划与实现约束。

## 目标能力

- `mmap`：建立用户虚拟地址到物理页/文件页的映射
- `munmap`：解除映射并回收资源
- `brk`：管理进程堆区边界

## 关键接口（规划）

```rust
pub fn sys_mmap(args: &SyscallArgs) -> SyscallRet;
pub fn sys_munmap(args: &SyscallArgs) -> SyscallRet;
pub fn do_mmap(mm: &mut MmStruct, req: &MmapRequest) -> Result<u64, MmError>;
pub fn do_munmap(mm: &mut MmStruct, addr: u64, len: u64) -> Result<(), MmError>;
```

## 相关文档

- [VMA API](./vma.md)
- [Paging API](./paging.md)
- [Fault API](./fault.md)
