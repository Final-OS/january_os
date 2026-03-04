# slub - SLUB 小对象分配器

SLUB (Simple Allocator of Unified Blocks) 是一个高效的小对象分配器，用于分配小于一页的内存块。

## 概述

SLUB 为不同大小维护多个缓存（cache），每个缓存包含多个 slab。

## 大小分类

```rust
const SLAB_SIZES: &[usize] = &[
    8, 16, 24, 32, 48, 64, 96, 128, 192, 256,
    384, 512, 768, 1024, 1536, 2048, 3072, 4096,
    6144, 8192,
];
```

## API

### 基本分配

```rust
pub fn kmalloc(size: usize, gfp_flags: GfpFlags) -> *mut u8
pub fn kzalloc(size: usize, gfp_flags: GfpFlags) -> *mut u8
pub fn kfree(ptr: *mut u8)
```

**参数**：
- `size`: 分配大小（字节）
- `gfp_flags`: 分配标志

**返回值**：分配的指针，失败返回空指针 `null`

**示例**：
```rust
use kernel::mm::{kmalloc, kfree, kzalloc, GFP_KERNEL};

// 分配 64 字节
let ptr = kmalloc(64, GFP_KERNEL);
if !ptr.is_null() {
    // 使用内存...
    kfree(ptr);
}

// 分配并清零
let ptr = kzalloc(128, GFP_KERNEL);
if !ptr.is_null() {
    // 内存已清零...
    kfree(ptr);
}
```

### 缓存管理

```rust
pub struct KmemCache {
    // ...
}

impl KmemCache {
    pub fn new(
        name: &str,
        size: usize,
        align: usize
    ) -> Result<Self, &'static str>

    pub fn alloc(&self, gfp_flags: GfpFlags) -> Option<*mut u8>
    pub fn free(&self, ptr: *mut u8)
    pub fn name(&self) -> &str
    pub fn object_size(&self) -> usize
    pub fn objects_per_slab(&self) -> usize
}
```

**示例**：
```rust
use kernel::mm::KmemCache;

// 创建自定义缓存
let cache = KmemCache::new("my_cache", 32, 8)?;

// 分配对象
if let Some(obj) = cache.alloc(GFP_KERNEL) {
    // 使用对象...
    cache.free(obj);
}

// 查询缓存信息
kprintln!("Cache: {}", cache.name());
kprintln!("Object size: {}", cache.object_size());
kprintln!("Objects per slab: {}", cache.objects_per_slab());
```

### 状态检查

```rust
pub fn slub_initialized() -> bool
pub fn kmalloc_stats() -> KmallocStats
```

`kmalloc_stats()` 聚合所有 `kmalloc-*` 缓存与大块分配（`alloc_pages` 路径）状态，供 `mm status` 使用。

## 数据结构

```
┌─────────────────────────────────────────────────────────┐
│                    KmemCache                             │
│  name: "kmalloc-64"                                     │
│  object_size: 64                                        │
│  objects_per_slab: 64                                   │
│  ┌─────────────────────────────────────────────────────┐│
│  │                  slab 列表                           ││
│  │  ┌─────────┐  ┌─────────┐  ┌─────────┐             ││
│  │  │ slab 0  │→│ slab 1  │→│ slab 2  │→ ...         ││
│  │  └─────────┘  └─────────┘  └─────────┘             ││
│  └─────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────┘

每个 slab:
┌─────────────────────────────────────────────────────────┐
│                      Slab                               │
│  ┌─────────────────────────────────────────────────────┐│
│  │           对象数组 (64 × 64 = 4096 bytes)           ││
│  │  ┌────┐┌────┐┌────┐ ...                            ││
│  │  │obj0││obj1││obj2│                                 ││
│  │  └────┘└────┘└────┘                                 ││
│  └─────────────────────────────────────────────────────┘│
│  free_list: obj0 → obj5 → obj12 → ...                  │
└─────────────────────────────────────────────────────────┘
```

## 分配流程

```
kmalloc(size, GFP_KERNEL)
    │
    ▼
查找合适的 cache
    │
    ▼
检查 Per-CPU slab
    │
    ├─ 有空闲对象 ──► 返回
    │
    └─ 无空闲对象 ──► 检查部分满 slab
                       │
                       ├─ 找到 ──► 移到 Per-CPU
                       │           │
                       │           └─► 返回对象
                       │
                       └─ 未找到 ──► 分配新 slab
                                       │
                                       └─► 返回对象
```

## 性能特点

1. **Per-CPU 缓存**：每个 CPU 独立的 slab，减少锁竞争
2. **对象复用**：释放的对象不立即返回到伙伴系统
3. **批量操作**：从伙伴系统批量分配/释放 slab
4. **内存对齐**：支持自定义对齐要求

## 使用场景

| 大小范围 | 用途 |
|----------|------|
| 8-32 字节 | 小结构体、指针 |
| 64-256 字节 | 中等结构体 |
| 512-4096 字节 | 大结构体、缓冲区 |
| 6144-8192 字节 | 接近页大小的分配 |

## 注意事项

1. **大小限制**：最大分配 8192 字节
2. **内存泄漏**：必须配对调用 kfree
3. **双重释放**：释放同一指针多次会崩溃
4. **对齐要求**：特殊对齐需求使用自定义 cache

## 代码示例

### 分配结构体

```rust
struct MyStruct {
    id: u64,
    name: [u8; 32],
    data: *mut u8,
}

fn alloc_my_struct() -> Option<&'static mut MyStruct> {
    let size = core::mem::size_of::<MyStruct>();
    let ptr = kmalloc(size, GFP_KERNEL)? as *mut MyStruct;
    unsafe {
        // 初始化结构体
        (*ptr).id = 0;
        (*ptr).name = [0; 32];
        (*ptr).data = core::ptr::null_mut();
        Some(&mut *ptr)
    }
}

fn free_my_struct(s: &mut MyStruct) {
    if !s.data.is_null() {
        kfree(s.data as *mut u8);
    }
    kfree(s as *mut MyStruct as *mut u8);
}
```

### 自定义 Cache

```rust
use kernel::mm::{KmemCache, GFP_KERNEL};

// 定义自定义对象
struct MyObject {
    value: u32,
    next: Option<*mut MyObject>,
}

// 创建全局缓存
static MY_CACHE: spinlock::SpinLock<Option<KmemCache>> =
    spinlock::SpinLock::new(None);

fn init_my_cache() {
    let cache = KmemCache::new(
        "my_objects",
        core::mem::size_of::<MyObject>(),
        core::mem::align_of::<MyObject>()
    ).unwrap();

    *MY_CACHE.lock() = Some(cache);
}

fn alloc_my_object() -> Option<*mut MyObject> {
    MY_CACHE.lock().as_ref()?.alloc(GFP_KERNEL)
        .map(|p| p as *mut MyObject)
}

fn free_my_object(obj: *mut MyObject) {
    if let Some(cache) = MY_CACHE.lock().as_ref() {
        cache.free(obj as *mut u8);
    }
}
```

## 相关文档

- [buddy - 伙伴系统](./buddy.md)
- [vmalloc - 虚拟连续分配](./vmalloc.md)
- [实现: 内存初始化](../../implementation/memory-init.md)
