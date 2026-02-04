# address - 地址类型

定义物理地址和虚拟地址类型，提供类型安全的地址操作。

## API

### 物理地址

```rust
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PhysAddr(pub u64);

impl PhysAddr {
    pub const fn new(addr: u64) -> Self
    pub fn as_u64(&self) -> u64
    pub fn as_ptr<T>(&self) -> *const T
    pub fn as_mut_ptr<T>(&self) -> *mut T
    pub fn is_null(&self) -> bool

    pub fn align_up(&self, align: u64) -> Self
    pub fn align_down(&self, align: u64) -> Self
    pub fn offset(&self, offset: u64) -> Self
    pub fn checked_add(&self, offset: u64) -> Option<Self>
}
```

### 虚拟地址

```rust
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VirtAddr(pub u64);

impl VirtAddr {
    pub const fn new(addr: u64) -> Self
    pub fn as_u64(&self) -> u64
    pub fn as_ptr<T>(&self) -> *const T
    pub fn as_mut_ptr<T>(&self) -> *mut T
    pub fn is_null(&self) -> bool

    pub fn align_up(&self, align: u64) -> Self
    pub fn align_down(&self, align: u64) -> Self
    pub fn offset(&self, offset: u64) -> Self
    pub fn checked_add(&self, offset: u64) -> Option<Self>
}
```

## 使用示例

### 创建地址

```rust
use kernel::mm::{PhysAddr, VirtAddr};

// 物理地址
let phys = PhysAddr::new(0x100000);

// 虚拟地址
let virt = VirtAddr::new(0xFFFF8000001000000);
```

### 地址运算

```rust
// 对齐
let aligned = phys.align_up(4096);  // 向上对齐到 4KB

// 偏移
let offset = phys.offset(0x1000);

// 加法（带检查）
let new_addr = phys.checked_add(0x1000)?;
```

### 指针转换

```rust
// 转换为指针
let ptr = virt.as_ptr::<u8>();
let mut_ptr = virt.as_mut_ptr::<u8>();

// 读写内存
unsafe {
    *mut_ptr = 0x42;
    let value = *ptr;
}
```

### 页操作

```rust
use kernel::mm::{page_to_pfn, pfn_to_page, PhysAddr, VirtAddr};

let phys = PhysAddr::new(0x1000);
let pfn = phys.as_u64 / 4096;

let page = pfn_to_page(pfn);
let phys = page_to_pfn(page) * 4096;

// 虚拟地址操作
let virt = VirtAddr::new(0xFFFF8000001000000);
let virt_phys = virt_to_phys(virt);
```

## 地址转换

```rust
// 物理到虚拟（通过直接映射）
pub fn phys_to_virt(phys: PhysAddr) -> VirtAddr {
    VirtAddr::new(phys.as_u64() + DIRECT_MAP_OFFSET)
}

// 虚拟到物理
pub fn virt_to_phys(virt: VirtAddr) -> Option<PhysAddr> {
    let addr = virt.as_u64();

    // 直接映射区
    if addr >= DIRECT_MAP_OFFSET && addr < DIRECT_MAP_END {
        return Some(PhysAddr::new(addr - DIRECT_MAP_OFFSET));
    }

    // 内核高半映射
    if addr >= KERNEL_VIRT_BASE {
        // 需要查页表...
    }

    None
}
```

## 常量地址

```rust
// kernel/src/mm/layout.rs

pub const DIRECT_MAP_OFFSET: u64 = 0xFFFF880000000000;
pub const DIRECT_MAP_END: u64 = 0xFFFFC80000000000;

pub const KERNEL_VIRT_BASE: u64 = 0xFFFF8000000000000;
pub const KERNEL_PHYS_BASE: u64 = 0x100000;

pub const VMALLOC_START: u64 = 0xFFFFC900000000000;
pub const VMALLOC_END: u64 = 0xFFFFF000FFFF0000;

pub const USER_MMAP_BASE: u64 = 0x7FFFF0000000;
```

## 页对齐

```rust
let addr = PhysAddr::new(0x1234);

// 向上对齐到 4KB
let aligned = addr.align_up(4096);

// 向下对齐到 4KB
let aligned = addr.align_down(4096);

// 检查是否对齐
let is_aligned = addr.as_u64() % 4096 == 0;
```

## 页大小常量

```rust
pub const PAGE_SIZE: u64 = 4096;
pub const PAGE_SHIFT: u64 = 12;

pub const PAGE_MASK: u64 = !(PAGE_SIZE - 1);

pub const HUGE_PAGE_SIZE: u64 = 2 * 1024 * 1024;  // 2MB
pub const HUGE_PAGE_SHIFT: u64 = 21;

pub const GIGANT_PAGE_SIZE: u64 = 1024 * 1024 * 1024; // 1GB
pub const GIGANT_PAGE_SHIFT: u64 = 30;
```

## 相关文档

- [VMA](./vma.md)
- [page](./page.md)
- [layout](./layout.md)
