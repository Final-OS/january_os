# zone - 内存区域管理

Zone 管理不同特性的内存区域，如 DMA、DMA32 和 Normal。

## API

### ZoneType

```rust
pub enum ZoneType {
    DMA,      // 0 - 16MB (传统 ISA DMA)
    DMA32,    // 1 - 16MB - 4GB (32-bit PCI DMA)
    Normal,   // 2 - 4GB+ (常规内存)
}

impl ZoneType {
    pub fn iter() -> impl Iterator<Item = ZoneType>
}
```

### Zone 结构

```rust
pub struct Zone {
    pub zone_type: ZoneType,
    pub start_pfn: u64,
    spanned_pages: u64,
    managed_pages: u64,
    present_pages: u64,
    nr_free_pages: AtomicU64,

    pub free_area: [SpinLock<FreeList>; MAX_ORDER + 1],
    pub lock: SpinLock<ZoneState>,
    pub watermark: ZoneWatermark,
    pub initialized: bool,
}
```

### Zone 操作

```rust
pub fn get_zone(zone_type: ZoneType) -> &'static Zone

impl Zone {
    pub fn nr_free_pages(&self) -> u64
    pub fn nr_managed_pages(&self) -> u64
    pub fn initialized(&self) -> bool
    pub fn is_initialized(&self) -> bool
}
```

### Zone 初始化

```rust
pub fn init_zone_buddy(
    zone_type: ZoneType,
    start_pfn: u64,
    end_pfn: u64
) -> Result<(), MmError>
```

### GFP 标志

```rust
pub struct GfpFlags: u32;

impl GfpFlags {
    pub const KERNEL: GfpFlags = GfpFlags::from_bits(0);
    pub const ATOMIC: GfpFlags = GfpFlags::from_bits(1);
    pub const DMA: GfpFlags = GfpFlags::from_bits(2);
    pub const DMA32: GfpFlags = GfpFlags::from_bits(3);
    pub const USER: GfpFlags = GfpFlags::from_bits(4);
    pub const ZERO: GfpFlags = GfpFlags::from_bits(5);
}

// 常用组合
pub const GFP_KERNEL: GfpFlags = GfpFlags::KERNEL;
pub const GFP_ATOMIC: GfpFlags = GfpFlags::ATOMIC;
pub const GFP_DMA: GfpFlags = GfpFlags::DMA;
pub const GFP_DMA32: GfpFlags = GfpFlags::DMA32;
pub const GFP_KERNEL_ZERO: GfpFlags = GfpFlags::KERNEL | GfpFlags::ZERO;
```

## 内存区域

| Zone | 地址范围 | 用途 |
|------|----------|------|
| ZONE_DMA | 0 - 16MB | 传统 ISA DMA (24-bit) |
| ZONE_DMA32 | 16MB - 4GB | 32-bit PCI DMA |
| ZONE_NORMAL | 4GB+ | 常规内存 |

## 水位线

```rust
pub struct ZoneWatermark {
    pub min: u64,
    pub low: u64,
    pub high: u64,
}
```

**计算**:
```rust
let managed_pages = zone.nr_managed_pages();

zone.watermark.min = managed_pages / 100;
zone.watermark.low = managed_pages / 50;
zone.watermark.high = managed_pages / 20;
```

## 使用示例

### 获取 Zone

```rust
use kernel::mm::{ZoneType, get_zone};

let zone = get_zone(ZoneType::Normal);

kprintln!("Free pages: {}", zone.nr_free_pages());
```

### 检查初始化状态

```rust
use kernel::mm::zones_initialized;

if zones_initialized() {
    kprintln!("All zones initialized");
} else {
    kprintln!("Some zones not initialized");
}
```

### GFP 标志使用

```rust
use kernel::mm::{alloc_pages, GFP_KERNEL, GFP_ATOMIC, GFP_DMA};

// 内核分配（可以睡眠）
let page1 = alloc_pages(2, GFP_KERNEL);

// 中断上下文（不能睡眠）
let page2 = alloc_pages(2, GFP_ATOMIC);

// DMA 分配
let page3 = alloc_pages(2, GFP_DMA);

// 分配并清零
let page4 = alloc_pages(2, GFP_KERNEL_ZERO);
```

## 工具函数

```rust
use kernel::mm::{pages_per_order, bytes_per_order, get_order};

// 计算 order 对应的页数
let pages = pages_per_order(3);  // 8 页

// 计算 order 对应的字节数
let bytes = bytes_per_order(3);  // 32768 字节

// 根据大小计算所需 order
let size = 16384;  // 16KB
let order = get_order(size);  // 2
```

## 常量

```rust
pub const MAX_ORDER: u32 = 11;

pub const NR_ZONES: usize = 3;

pub const GFP_KERNEL: GfpFlags = GfpFlags::from_bits(0);

// Zone 大小限制
pub const DMA_LIMIT: u64 = 16777216;         // 16 MB
pub const DMA32_LIMIT: u64 = 4294967296;      // 4 GB
```

## 相关文档

- [buddy](./buddy.md)
- [pcp](./pcp.md)
- [numa](./numa.md)
