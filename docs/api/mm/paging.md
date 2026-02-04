# paging - 页表操作

页表管理模块提供虚拟地址到物理地址映射的底层操作。

## API

### PageTable 条目

```rust
pub struct PageTableEntry(pub u64);

impl PageTableEntry {
    pub fn new() -> Self
    pub fn is_present(&self) -> bool
    pub fn is_huge(&self) -> bool
    pub fn is_writable(&self) -> bool
    pub fn is_user(&self) -> bool
    pub fn is_no_execute(&self) -> bool

    pub fn set_present(&mut self, value: bool)
    pub fn set_writable(&mut self, value: bool)
    pub fn set_user(&mut self, value: bool)
    pub fn set_huge(&mut self, value: bool)
    pub fn set_global(&mut self, value: bool)
    pub fn set_no_execute(&mut self, value: bool)

    pub fn address(&self) -> PhysAddr
    pub fn set_address(&mut self, addr: PhysAddr)

    pub fn flags(&self) -> PageTableFlags
}
```

### 页表标志

```rust
pub struct PageTableFlags: u64 {
    const PRESENT   = 1 << 0;  // 页存在
    const WRITABLE  = 1 << 1;  // 可写
    const USER      = 1 << 2;  // 用户可访问
    const PWT       = 1 << 3;  // 页级直写
    const PCD       = 1 << 4;  // 禁用缓存
    const ACCESSED  = 1 << 5;  // 已访问
    const DIRTY     = 1 << 6;  // 脏
    const GLOBAL    = 1 << 8;  // 全局页
    const NO_EXECUTE = 1 << 63; // 禁止执行
}
```

### PageTable

```rust
pub struct PageTable {
    pml4: PhysAddr,
}
```

### PageTableManager

```rust
pub struct PageTableManager {
    // ...
}

impl PageTableManager {
    pub fn new(pml4_phys: PhysAddr) -> Self
    pub fn map_page(&mut self, virt: VirtAddr, phys: PhysAddr, flags: PageTableFlags)
    pub fn unmap_page(&mut self, virt: VirtAddr)
    pub fn map_pages(&mut self, virt: VirtAddr, phys: PhysAddr, count: usize, flags: PageTableFlags)
    pub fn get_entry(&mut self, virt: VirtAddr) -> Option<&mut PageTableEntry>
}
```

## 使用示例

### 创建页表

```rust
use kernel::mm::paging::{PageTable, PageTableManager};

// 创建新的页表 (PML4)
let pml4_page = alloc_pages(1, GFP_KERNEL)?;
let pml4_addr = page_to_pfn(pml4_page) * 4096;
let pt = PageTable::new(PhysAddr::new(pml4_addr));
```

### 映射页面

```rust
let mut ptm = PageTableManager::new(pml4_phys_addr);

// 映射单页
let virt = VirtAddr::new(0xFFFF8000001000000);
let phys = PhysAddr::new(0x100000);
ptm.map_page(virt, phys, PageTableFlags::PRESENT | PageTableFlags::WRITABLE);

// 映射多页
ptm.map_pages(virt, phys, 10, flags);
```

### 设置页表项

```rust
use kernel::mm::paging::{PageTableFlags, PhysAddr, VirtAddr};

let entry = PageTableEntry::new();

entry.set_present(true);
entry.set_writable(true);
entry.set_user(true);
entry.set_address(PhysAddr::new(0x100000));

// 4KB 页对齐地址
```

## TLB 操作

```rust
// 刷新 TLB
pub fn invlpg(virt: VirtAddr)

// 刷新整个 TLB (需要加载新的 CR3)
pub fn reload_cr3()
```

**示例**:
```rust
// 刷新单页 TLB
unsafe {
    invlpg(virt_addr.as_u64());
}

// 刷新整个 TLB
unsafe {
    reload_cr3();
}
```

## 地址转换

```rust
// 物理到虚拟
pub fn phys_to_virt(phys: PhysAddr) -> VirtAddr {
    VirtAddr::new(phys.as_u64() + DIRECT_MAP_OFFSET)
}

// 虚拟到物理
pub fn virt_to_phys(virt: VirtAddr) -> Option<PhysAddr> {
    let addr = virt.as_u64();
    if addr >= DIRECT_MAP_OFFSET {
        Some(PhysAddr::new(addr - DIRECT_MAP_OFFSET))
    } else if addr >= KERNEL_VIRT_BASE {
        // 查页表...
    } else {
        None
    }
}
```

## 页表层级

```
PML4 (Page Map Level 4) - 512 项
    │
    ├──> PDPT (Page Directory Pointer Table)
    │       │
    │       ├──> PD (Page Directory) - 512 项
    │       │       │
    │       │       ├──> PT (Page Table) - 512 项
    │       │       │       │
    │       │       │       └─> 4KB 页
    │       │       │
    │       │       └─> 2MB 大页 (如果设置 huge)
    │       │
    │       └─> 1GB 巨大页 (如果设置 huge)
```

## 大页支持

```rust
// 2MB 大页 (PD 级)
entry.set_huge(true);
entry.set_address(PhysAddr::new(0x200000));  // 必须对齐

// 1GB 跨大页 (PDPT 级)
entry.set_huge(true);
entry.set_address(PhysAddr::new(0x40000000));  // 必须对齐
```

## 相关文档

- [VMA](./vma.md)
- [vmalloc](./vmalloc.md)
- [fault](./fault.md)
- [引导流程](../../implementation/boot.md)
