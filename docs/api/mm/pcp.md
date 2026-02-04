# pcp - Per-CPU Page Cache

PCP (Per-CPU Page Cache) 为每个 CPU 维护独立的页缓存，减少 Buddy 系统的锁竞争。

## 概述

PCP 在每个 CPU 上缓存热点页，避免频繁访问 Buddy 系统。

## API

### 初始化

```rust
pub fn init_pcp(batch_size: usize)
pub fn pcp_initialized() -> bool
```

初始化 PCP 系统。

**参数**：
- `batch_size`: 批量分配/释放的页数

**示例**：
```rust
use kernel::mm::init_pcp;

init_pcp(16); // 批量操作 16 页
```

### 分配/释放

```rust
pub fn pcp_alloc_page(
    order: u32,
    gfp_flags: GfpFlags
) -> Option<Page>

pub fn pcp_free_page(page: Page, order: u32)
```

**示例**：
```rust
use kernel::mm::{pcp_alloc_page, pcp_free_page, GFP_KERNEL};

// 从 PCP 分配
if let Some(page) = pcp_alloc_page(0, GFP_KERNEL) {
    // 使用页面...
    pcp_free_page(page, 0);
}
```

### 排空

```rust
pub fn drain_all_pcps()
```

排空所有 CPU 的 PCP 缓存，将页面返回给 Buddy 系统。

**使用场景**：
- 内存不足时
- CPU 下线时
- 系统关机时

### 统计

```rust
pub fn pcp_stats() -> PcpStats

pub struct PcpStats {
    pub cpu_id: u32,
    pub alloc_count: u64,
    pub free_count: u64,
    pub high: u32,
    pub batch: u32,
}
```

**示例**：
```rust
use kernel::mm::pcp_stats;

for cpu in 0..4 {
    if let Some(stats) = pcp_stats_for_cpu(cpu) {
        kprintln!("CPU {}: alloc={} free={}",
            cpu,
            stats.alloc_count,
            stats.free_count);
    }
}
```

## 配置

### 配置文件

```toml
# os_cfg.toml
[memory.pcp]
high_watermark = 64   # 单 CPU 缓存上限
batch_size = 16       # 批量操作大小
```

### 水位线

```
PCP 缓存状态:

high (64)
    │
    │  ┌───────────────────┐
    │  │  缓存充足        │
    │  └───────────────────┘
low (high - batch = 48)
    │
    │  ┌───────────────────┐
    │  │  需要补充        │
    │  └───────────────────┘
    │
    └── empty (0)
```

## 工作原理

### 分配流程

```
pcp_alloc_page(order, GFP_KERNEL)
    │
    ▼
检查当前 CPU 的 PCP 缓存
    │
    ├─ 有页 ──► 从缓存取
    │              │
    │              └─► 检查水位线
    │                      │
    │                      ├─ 低于 low ──► 从 Buddy 批量补充
    │                      │
    │                      └─ 高于 low ──► 继续
    │
    └─ 无页 ──► 从 Buddy 批量分配
                    │
                    └─► 返回一页
```

### 释放流程

```
pcp_free_page(page, order)
    │
    ▼
检查当前 CPU 的 PCP 缓存
    │
    ├─ 未满 ──► 添加到缓存
    │
    └─ 已满 ──► 批量释放到 Buddy
                    │
                    └─► 清空部分缓存
```

## 数据结构

```rust
struct PerCpuPage {
    list: ListHead,      // 页链表
    count: u32,         // 当前页数
    high: u32,          // 水位上限
    batch: u32,         // 批量大小
}

struct PcpZone {
    pages: [PerCpuPage; MAX_ORDER],
    // 每个 order 一个列表
}
```

## 性能优势

| 场景 | 无 PCP | 有 PCP |
|------|--------|--------|
| 单页分配 | 需要获取 Buddy 锁 | 从本地缓存分配 |
| 多核竞争 | 锁竞争严重 | 无竞争 |
| 分配速度 | 较慢 | 快速 |

## 使用场景

1. **高频分配**：SLUB 分配器底层使用
2. **多核系统**：减少 SMP 系统的锁竞争
3. **实时应用**：减少分配延迟

## 注意事项

1. **内存开销**：每个 CPU 有独立缓存
2. **不平衡**：不同 CPU 的缓存可能不均衡
3. **排空成本**：drain_all_pcps() 有一定开销

## 相关文档

- [buddy - 伙伴系统](./buddy.md)
- [slub - 小对象分配器](./slub.md)
- [实现: 内存初始化](../../implementation/memory-init.md)
