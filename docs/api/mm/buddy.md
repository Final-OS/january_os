# buddy - 伙伴系统分配器

伙伴系统是一种高效管理物理内存的算法，通过将内存块分成大小为 2^order 页的块来管理外碎片。

## 概述

伙伴系统维护每个 Zone 的空闲列表，每个 order 对应一个列表。

## Order 大小

| Order | 页数 | 大小 |
|-------|------|------|
| 0 | 1 | 4KB |
| 1 | 2 | 8KB |
| 2 | 4 | 16KB |
| 3 | 8 | 32KB |
| 4 | 16 | 64KB |
| 5 | 32 | 128KB |
| 6 | 64 | 256KB |
| 7 | 128 | 512KB |
| 8 | 256 | 1MB |
| 9 | 512 | 2MB |
| 10 | 1024 | 4MB |
| 11 | 2048 | 8MB |

## API

### 初始化

```rust
pub unsafe fn init_buddy_system(
    regions: &[MemoryRegionInfo],
    max_pfn: u64,
    direct_map_offset: u64
) -> Result<(), &'static str>
```

初始化伙伴系统。

**参数**：
- `regions`: 内存区域信息
- `max_pfn`: 最大页帧号
- `direct_map_offset`: 直接映射偏移

### 分配页面

```rust
pub fn alloc_pages(order: u32, gfp_flags: GfpFlags) -> Option<Page>
pub fn alloc_page(gfp_flags: GfpFlags) -> Option<Page>
```

分配 2^order 个连续页。

**参数**：
- `order`: 分配的 order (0-11)
- `gfp_flags`: 分配标志

**返回值**：分配的 `Page`，失败返回 `None`

**示例**：
```rust
use kernel::mm::{alloc_pages, alloc_page, GFP_KERNEL, GFP_ATOMIC};

// 分配 8 页 (order=3, 32KB)
if let Some(page) = alloc_pages(3, GFP_KERNEL) {
    let pfn = page_to_pfn(page);
    kprintln!("Allocated PFN: {}", pfn);
}

// 分配单页
if let Some(page) = alloc_page(GFP_ATOMIC) {
    // ...
}
```

### 释放页面

```rust
pub fn free_pages(page: Page, order: u32)
pub fn free_page(page: Page)
```

释放页面回伙伴系统。

**参数**：
- `page`: 要释放的页面
- `order`: 之前分配的 order

**示例**：
```rust
if let Some(page) = alloc_pages(3, GFP_KERNEL) {
    // 使用页面...
    free_pages(page, 3);
}
```

### Zone 初始化

```rust
pub fn init_zone_buddy(
    zone_type: ZoneType,
    start_pfn: u64,
    end_pfn: u64
) -> Result<(), &'static str>
```

初始化指定 Zone 的伙伴系统。

## GFP 标志

```rust
pub struct GfpFlags: u32 {
    const KERNEL = 1 << 0;      // 内核分配，可能睡眠
    const ATOMIC = 1 << 1;      // 原子分配，不能睡眠
    const DMA    = 1 << 2;      // 从 ZONE_DMA 分配
    const DMA32  = 1 << 3;      // 从 ZONE_DMA32 分配
    const USER   = 1 << 4;      // 用户空间分配
    const ZERO   = 1 << 5;      // 分配并清零
}

// 常用组合
pub const GFP_KERNEL: GfpFlags = GfpFlags::KERNEL;
pub const GFP_ATOMIC: GfpFlags = GfpFlags::ATOMIC;
pub const GFP_DMA: GfpFlags = GfpFlags::DMA;
pub const GFP_DMA32: GfpFlags = GfpFlags::DMA32;
pub const GFP_KERNEL_ZERO: GfpFlags = GfpFlags::KERNEL | GfpFlags::ZERO;
```

**使用示例**：
```rust
// 内核分配（可以睡眠）
let page = alloc_pages(2, GFP_KERNEL);

// 中断上下文（不能睡眠）
let page = alloc_pages(1, GFP_ATOMIC);

// DMA 分配
let page = alloc_pages(2, GFP_DMA);

// 分配并清零
let page = alloc_pages(2, GFP_KERNEL_ZERO);
```

## 工具函数

```rust
pub fn pages_per_order(order: u32) -> u64
pub fn bytes_per_order(order: u32) -> u64
pub fn get_order(size: u64) -> u32
```

**示例**：
```rust
// 计算 order 对应的页数
let pages = pages_per_order(3); // 8

// 计算 order 对应的字节数
let bytes = bytes_per_order(3); // 32768

// 根据大小计算所需 order
let size = 16384; // 16KB
let order = get_order(size); // 2
```

## Zone 管理

```rust
pub fn get_zone(zone_type: ZoneType) -> &'static Zone
```

**Zone 类型**：
```rust
pub enum ZoneType {
    DMA,      // 0 - 16MB
    DMA32,    // 16MB - 4GB
    Normal,   // 4GB+
}
```

**示例**：
```rust
let zone = get_zone(ZoneType::Normal);
let free = zone.nr_free_pages();
let managed = zone.nr_managed_pages();

kprintln!("Zone: free={} managed={}", free, managed);
```

## 状态检查

```rust
pub fn zones_initialized() -> bool
```

检查所有 Zone 是否已初始化。

## 分配流程

```
alloc_pages(order, GFP_KERNEL)
    │
    ▼
根据 GFP 标志选择 Zone
    │
    ▼
检查对应 order 的空闲列表
    │
    ├─ 有空闲块 ──► 返回
    │
    └─ 无空闲块 ──► 查找更高 order
                       │
                       ├─ 找到 ──► 分裂块
                       │           │
                       │           └─► 返回一半
                       │
                       └─ 未找到 ──► 返回 None
```

## 释放流程

```
free_pages(page, order)
    │
    ▼
检查 buddy 是否空闲
    │
    ├─ 是 ──► 合并为更大块
    │           │
    │           └─► 递归检查
    │
    └─ 否 ──► 添加到空闲列表
```

## 性能考虑

1. **外碎片控制**：伙伴系统自动合并相邻空闲块
2. **分配速度**：O(order) 时间复杂度
3. **内存开销**：每个 Page 结构约 64 字节
4. **锁竞争**：每个 Zone 独立锁，减少竞争

## 注意事项

1. **order 匹配**：释放时必须使用分配时的 order
2. **Zone 限制**：GFP_DMA/GFP_DMA32 会限制分配范围
3. **原子性**：GFP_ATOMIC 保证不睡眠，适合中断上下文
4. **清零**：需要清零页面时使用 GFP_KERNEL_ZERO

## 相关文档

- [memblock - 早期分配器](./memblock.md)
- [slub - 小对象分配器](./slub.md)
- [zone - Zone 管理](./zone.md)
- [page - 页帧描述符](./page.md)
