# vma - 虚拟内存区域

VMA (Virtual Memory Area) 管理进程的虚拟地址空间，表示一段连续的虚拟内存区域。

## 概述

VMA 描述虚拟地址空间中的一个区域，包含起始地址、大小和访问权限。

## API

### VMA 结构

```rust
pub struct Vma {
    pub start: VirtAddr,
    pub end: VirtAddr,
    pub flags: VmFlags,
    pub vm_file: Option<()>,  // TODO: 文件映射
    pub vm_next: Option<&'static Vma>,
}

pub struct VmFlags: u32 {
    const READ   = 1 << 0;
    const WRITE  = 1 << 1;
    const EXEC   = 1 << 2;
    const SHARED = 1 << 3;
    const MAYREAD   = 1 << 4;
    const MAYWRITE  = 1 << 5;
    const MAYEXEC   = 1 << 6;
    const GROWSDOWN = 1 << 7;  // 向下增长 (栈)
    const GROWSUP   = 1 << 8;  // 向上增长
}
```

### VMA 操作

```rust
impl Vma {
    pub fn new(
        start: VirtAddr,
        size: u64,
        flags: VmFlags
    ) -> Self

    pub fn contains(&self, addr: VirtAddr) -> bool
    pub fn overlaps(&self, other: &Vma) -> bool
    pub fn can_merge(&self, other: &Vma) -> bool
}
```

**示例**：
```rust
use kernel::mm::{Vma, VmFlags, VirtAddr};

// 创建可读写 VMA
let vma = Vma::new(
    VirtAddr::new(0x40000000),
    0x1000,
    VmFlags::READ | VmFlags::WRITE
);

// 检查地址是否在 VMA 中
let addr = VirtAddr::new(0x40000500);
if vma.contains(addr) {
    kprintln!("Address in VMA");
}
```

### Mm 结构（内存描述符）

```rust
pub struct Mm {
    pub vmas: ListHead,      // VMA 链表
    pub map_count: u32,      // VMA 数量
    pub total_vm: u64,       // 总页数
    pub locked_vm: u64,      // 锁定页数
    pub start_brk: VirtAddr,
    pub brk: VirtAddr,
    pub start_stack: VirtAddr,
}
```

### Mm 操作

```rust
impl Mm {
    pub fn mmap(
        &self,
        vma: &Vma
    ) -> Result<(), VmaError>

    pub fn munmap(
        &self,
        addr: VirtAddr,
        size: u64
    ) -> Result<(), VmaError>

    pub fn find_vma(
        &self,
        addr: VirtAddr
    ) -> Option<&Vma>

    pub fn find_vma_intersection(
        &self,
        start: VirtAddr,
        end: VirtAddr
    ) -> Option<&Vma>
}
```

**示例**：
```rust
use kernel::mm::{get_init_mm, Vma, VmFlags, VirtAddr};

let mm = get_init_mm();

// 映射新区域
let vma = Vma::new(
    VirtAddr::new(0x40000000),
    0x1000,
    VmFlags::READ | VmFlags::WRITE
);

if let Err(e) = mm.mmap(&vma) {
    kprintln!("mmap failed: {:?}", e);
}

// 查找 VMA
if let Some(vma) = mm.find_vma(VirtAddr::new(0x40000500)) {
    kprintln!("Found VMA: {:#x} - {:#x}, flags: {:?}",
        vma.start.as_u64(),
        vma.end.as_u64(),
        vma.flags);
}

// 解除映射
mm.munmap(VirtAddr::new(0x40000000), 0x1000).ok();
```

### 初始化

```rust
pub fn init_vma()
pub fn get_init_mm() -> &'static Mm
```

## 标志转换

### mmap 到 VmFlags

```rust
pub fn mmap_flags_to_vm_flags(flags: u32) -> VmFlags

pub const MAP_SHARED: u32 = 0x01;
pub const MAP_PRIVATE: u32 = 0x02;
pub const MAP_FIXED: u32 = 0x10;
pub const MAP_ANONYMOUS: u32 = 0x20;
pub const MAP_GROWSDOWN: u32 = 0x100;
```

### prot 到 VmFlags

```rust
pub fn prot_flags_to_vm_flags(prot: u32) -> VmFlags

pub const PROT_READ: u32 = 0x1;
pub const PROT_WRITE: u32 = 0x2;
pub const PROT_EXEC: u32 = 0x4;
```

**示例**：
```rust
use kernel::mm::{mmap_flags_to_vm_flags, prot_flags_to_vm_flags};

// mmap 标志
let vm_flags = mmap_flags_to_vm_flags(MAP_PRIVATE | MAP_ANONYMOUS);

// prot 标志
let vm_flags = prot_flags_to_vm_flags(PROT_READ | PROT_WRITE);
```

## VMA 链表

VMA 按地址排序，通过红黑树（规划）和链表组织：

```
mm->mmap
    │
    ▼
┌──────────┐
│ VMA 1    │
│ 0x1000   │──► ┌──────────┐
│ - 0x2000 │    │ VMA 2    │
└──────────┘    │ 0x2000   │──► ...
                │ - 0x3000 │
                └──────────┘
```

## 使用场景

### 栈区域

```rust
// 栈 VMA，向下增长
let stack_vma = Vma::new(
    VirtAddr::new(0x7ffffffff000 - 0x1000),
    0x1000,
    VmFlags::READ | VmFlags::WRITE | VmFlags::GROWSDOWN
);
```

### 堆区域

```rust
// 堆 VMA，向上增长
let heap_vma = Vma::new(
    VirtAddr::new(0x40000000),
    0x1000,
    VmFlags::READ | VmFlags::WRITE | VmFlags::GROWSUP
);
```

### 代码段

```rust
// 代码段，只读可执行
let code_vma = Vma::new(
    VirtAddr::new(0x40000000),
    0x1000,
    VmFlags::READ | VmFlags::EXEC
);
```

## 错误处理

```rust
pub enum VmaError {
    InvalidAddress,
    Overlap,
    PermissionDenied,
    OutOfMemory,
}
```

## 相关文档

- [vmalloc - 虚拟连续分配](./vmalloc.md)
- [fault - 页错误处理](./fault.md)
- [实现: 内存初始化](../../implementation/memory-init.md)
