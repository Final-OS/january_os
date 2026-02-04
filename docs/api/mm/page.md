# page - 页帧描述符

每个物理页帧对应一个 `struct page` 结构，用于跟踪页的状态。

## API

### Page 结构

```rust
pub struct Page {
    pub flags: AtomicU64,      // 页标志
    pub _refcount: AtomicU16,  // 引用计数
    pub _order: u8,            // 当前 order
    pub _list_head: ListHead,  // 链表节点
}
```

### PageFlags

```rust
pub struct PageFlags: u64 {
    const RESERVED   = 1 << 0;  // 已保留
    const BUDDY      = 1 << 1;  // 伙伴系统管理
    const SLUB       = 1 << 2;  // SLUB 管理
    const DMA        = 1 << 3;  // DMA 页
    const MAPPED     = 1 << 4;  // 已映射
    const RECLAIM    = 1 << 5;  // 可回收
}
```

### PFN 与 Page 转换

```rust
pub fn pfn_to_page(pfn: u64) -> &'static Page
pub fn page_to_pfn(page: &Page) -> u64

pub const PAGE_STRUCT_SIZE: usize = 64;
```

## 使用示例

### 转换 PFN 和 Page

```rust
use kernel::mm::{page_to_pfn, pfn_to_page};

// PFN 转 Page
let pfn = 256;
let page = pfn_to_page(pfn);

// Page 转 PFN
let pfn = page_to_pfn(page);
```

### 检查页标志

```rust
use kernel::mm::{PageFlags, page_to_pfn};

let page = pfn_to_page(256);

// 检查是否被保留
if page.flags.load(Ordering::Relaxed).contains(PageFlags::RESERVED) {
    kprintln!("Page is reserved");
}

// 检查是否在伙伴系统中
if page.flags.load(Ordering::Relaxed).contains(PageFlags::BUDDY) {
    kprintln!("Page is managed by buddy");
}
```

### 设置页标志

```rust
use kernel::mm::{PageFlags, pfn_to_page};

let page = pfn_to_page(256);
let mut flags = PageFlags::empty();

flags |= PageFlags::RESERVED;
flags |= PageFlags::MAPPED;

page.flags.store(flags.bits(), Ordering::Release);
```

### 引用计数

```rust
// 增加引用
page._refcount.fetch_add(1, Ordering::Relaxed);

// 减少引用
if page._refcount.fetch_sub(1, Ordering::Release) == 1 {
    // 引用计数为 0，可以释放
}
```

### 初始化 vmemmap

```rust
pub fn init_vmemmap(start_pfn: u64, end_pfn: u64)
```

初始化所有页的描述符：

```rust
pub fn init_vmemmap(start_pfn: u64, end_pfn: u64) {
    for pfn in start_pfn..end_pfn {
        let page = pfn_to_page(pfn);

        // 设置初始标志
        page.flags.store(PageFlags::RESERVED.bits(), Ordering::Relaxed);
        page._refcount.store(1, Ordering::Relaxed);
        page._order = 0;
    }
}
```

## 内存布局

```
struct page 数组
┌─────────────────────────────────────────────────────────┐
│  Page 0   │  Page 1   │  Page 2   │  ...  Page N-1      │
│  (PFN 0)   │  (PFN 1)   │  (PFN 2)   │                    │
└─────────────────────────────────────────────────────────┘

每个 Page 结构大小 = 64 字节
```

## 常量

```rust
// 页大小相关
pub const PAGE_SIZE: u64 = 4096;
pub const PAGE_SHIFT: u64 = 12;
pub const PAGE_MASK: u64 = !(PAGE_SIZE - 1);

// struct page 大小
pub const PAGE_STRUCT_SIZE: usize = 64;

// 地址计算
pub const MAX_PHYSICAL_ADDRESS: u64 = 0x800000000;  // 128 GB
pub const MAX_PFN: u64 = MAX_PHYSICAL_ADDRESS / PAGE_SIZE;
```

## 相关文档

- [buddy](./buddy.md)
- [zone](./zone.md)
- [address](./address.md)
