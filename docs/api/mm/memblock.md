# memblock - 早期引导内存分配器

memblock 是一个简单的早期引导内存分配器，在页表和伙伴系统初始化之前使用。

## 概述

memblock 管理物理内存的两个列表：
- **memory** - 可用内存区域
- **reserved** - 保留/已分配的内存区域

## API

### 初始化

```rust
pub unsafe fn memblock_init(
    regions: &[MemoryRegionInfo],
    kernel_start: u64,
    kernel_end: u64
) -> Result<(), &'static str>
```

初始化 memblock 分配器。

**参数**：
- `regions`: 内存区域信息数组
- `kernel_start`: 内核起始物理地址
- `kernel_end`: 内核结束物理地址

**示例**：
```rust
let regions = &[
    MemoryRegionInfo {
        phys_start: 0x1000000,
        page_count: 16384,
        is_usable: true,
    },
    // ...
];

unsafe {
    memblock_init(regions, 0x100000, 0x200000)?;
}
```

### 添加内存

```rust
pub fn memblock_add(base: u64, size: u64)
pub fn memblock_reserve(base: u64, size: u64)
```

**参数**：
- `base`: 物理基地址
- `size`: 大小（字节）

**示例**：
```rust
// 添加可用内存
memblock_add(0x10000000, 0x10000000);

// 保留内存区域
memblock_reserve(0x100000, 0x10000); // 内核
```

### 分配内存

```rust
pub fn memblock_alloc(size: u64, align: u64) -> Option<u64>
pub fn memblock_alloc_range(
    start: u64,
    end: u64,
    size: u64,
    align: u64
) -> Option<u64>
pub fn memblock_alloc_zeroed(size: u64, align: u64) -> Option<u64>
```

**返回值**：分配的物理地址，失败返回 `None`

**示例**：
```rust
// 分配 16KB，16 字节对齐
let addr = memblock_alloc(16384, 16)?;

// 在指定范围内分配
let addr = memblock_alloc_range(0x10000000, 0x20000000, 4096, 4096)?;

// 分配并清零
let addr = memblock_alloc_zeroed(4096, 4096)?;
```

### 释放内存

```rust
pub fn memblock_free(base: u64, size: u64)
```

**参数**：
- `base`: 要释放的物理地址
- `size`: 大小（字节）

### 查询

```rust
pub fn memblock_phys_mem_size() -> u64
pub fn memblock_reserved_size() -> u64
pub fn memblock_free_size() -> u64
pub fn memblock_end_of_phys_mem() -> u64
pub fn memblock_initialized() -> bool
```

**示例**：
```rust
let total = memblock_phys_mem_size();     // 总物理内存
let reserved = memblock_reserved_size();  // 保留内存
let free = memblock_free_size();          // 可用内存
let end = memblock_end_of_phys_mem();     // 最大物理地址
```

### 遍历

```rust
pub fn memblock_for_each_free_region<F>(f: F)
where
    F: FnMut(&MemblockRegion)
```

**示例**：
```rust
memblock_for_each_free_region(|region| {
    kprintln!("Free: {:#x} - {:#x}",
        region.base,
        region.base + region.size - 1);
});
```

### 区域计数

```rust
pub fn memblock_memory_region_count() -> usize
pub fn memblock_reserved_region_count() -> usize
```

### 获取区域

```rust
pub fn memblock_memory_region(index: usize) -> Option<&'static MemblockRegion>
pub fn memblock_reserved_region(index: usize) -> Option<&'static MemblockRegion>
```

### 方向控制

```rust
pub fn memblock_set_bottom_up(val: bool)
pub fn memblock_set_current_limit(limit: u64)
```

**示例**：
```rust
// 设置分配方向
memblock_set_bottom_up(true);  // 从低地址向上分配

// 设置分配上限
memblock_set_current_limit(0x100000000); // 最多分配到 4GB
```

## 数据结构

```rust
pub struct MemblockRegion {
    pub base: u64,   // 基地址
    pub size: u64,   // 大小
    pub flags: MemblockFlags,
}

pub struct MemblockFlags: u32 {
    const RESERVED  = 1 << 0;
    const NOMAP     = 1 << 1;
}
```

## 使用场景

1. **早期引导**：在页表初始化之前分配内存
2. **内核加载**：为内核镜像预留内存
3. **初始化数据结构**：分配伙伴系统所需的数据结构
4. **过渡到伙伴系统**：初始化完成后不再使用

## 注意事项

- memblock 是线性分配器，释放后不会合并
- 在伙伴系统初始化后应停止使用
- 不支持分配后的回收（除了保留标记）
- 适合短期、低频率的内存分配

## 相关文档

- [buddy - 伙伴系统](./buddy.md)
- [slub - 小对象分配器](./slub.md)
- [实现: 内存初始化](../../implementation/memory-init.md)
