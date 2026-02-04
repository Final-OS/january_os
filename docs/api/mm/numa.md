# numa - NUMA 支持

NUMA (Non-Uniform Memory Access) 支持多节点内存管理，优化内存访问性能。

## 概述

NUMA 系统中，不同 CPU 访问不同内存节点的延迟不同。

## API

### 初始化

```rust
pub fn init_numa(srat: &Srat) -> Result<(), NumaError>
pub fn init_uma()
pub fn numa_initialized() -> bool
```

**SRAT (System Resource Affinity Table)**：描述 NUMA 拓扑

**示例**：
```rust
use kernel::mm::numa::{init_numa, init_uma, numa_initialized};

// 从 ACPI SRAT 初始化 NUMA
if let Ok(srat) = acpi::get_table::<Srat>(ACPI_SRAT_SIGNATURE) {
    init_numa(&srat)?;
} else {
    // 回退到 UMA
    init_uma();
}
```

### 节点查询

```rust
pub fn numa_node_id() -> u32
pub fn nr_online_nodes() -> u32
pub fn is_numa() -> bool
pub fn node_online(node: u32) -> bool
```

**示例**：
```rust
use kernel::mm::numa::{numa_node_id, nr_online_nodes, is_numa};

if is_numa() {
    let current_node = numa_node_id();
    let total_nodes = nr_online_nodes();
    kprintln!("Node: {} / {}", current_node, total_nodes);
}
```

### 距离查询

```rust
pub fn numa_distance(from: u32, to: u32) -> u32

pub const LOCAL_DISTANCE: u32 = 10;
pub const REMOTE_DISTANCE: u32 = 20;
```

**示例**：
```rust
use kernel::mm::numa::{numa_distance, LOCAL_DISTANCE, REMOTE_DISTANCE};

let node0 = 0;
let node1 = 1;

let dist = numa_distance(node0, node1);
if dist == LOCAL_DISTANCE {
    kprintln!("Same node");
} else if dist == REMOTE_DISTANCE {
    kprintln!("Different node");
}
```

### 节点选择

```rust
pub enum NumaPolicy {
    MPOL_DEFAULT,     // 默认策略
    MPOL_BIND,        // 绑定到节点
    MPOL_INTERLEAVE,  // 交错分配
    MPOL_PREFERRED,   // 首选节点
}

pub fn select_node(policy: NumaPolicy) -> Option<u32>
pub fn get_fallback_nodes(node: u32) -> Vec<u32>
```

**示例**：
```rust
use kernel::mm::numa::{select_node, NumaPolicy};

// 交错分配到所有节点
let node = select_node(NumaPolicy::MPOL_INTERLEAVE)?;

// 获取回退节点列表
let fallback = get_fallback_nodes(current_node);
```

### 节点数据

```rust
pub struct PgData {
    pub node_id: u32,
    pub node_start_pfn: u64,
    pub node_spanned_pages: u64,
    pub zones: [Zone; NR_ZONES],
}

pub fn get_node_data(node: u32) -> Option<&'static PgData>
```

**示例**：
```rust
use kernel::mm::numa::get_node_data;

if let Some(pgdata) = get_node_data(0) {
    kprintln!("Node 0: {} pages",
        pgdata.node_spanned_pages);
}
```

### 统计

```rust
pub fn numa_stats() -> NumaStats

pub struct NumaStats {
    pub total_nodes: u32,
    pub online_nodes: u32,
    pub total_memory: u64,
    pub node_memory: Vec<u64>,
}
```

## 配置

```toml
# os_cfg.toml
[memory_model]
type = "uma"  # "uma" 或 "numa"

[memory_model.numa]
max_nodes = 8
```

## 拓扑示例

```
2-socket NUMA 系统:

┌─────────────────────┐     ┌─────────────────────┐
│     Node 0          │     │     Node 1          │
│  ┌───────────────┐  │     │  ┌───────────────┐  │
│  │ CPU 0, CPU 1  │  │     │  │ CPU 2, CPU 3  │  │
│  └───────────────┘  │     │  └───────────────┘  │
│  ┌───────────────┐  │     │  ┌───────────────┐  │
│  │   Memory      │  │     │  │   Memory      │  │
│  │   8 GB        │  │     │  │   8 GB        │  │
│  └───────────────┘  │     │  └───────────────┘  │
└─────────────────────┘     └─────────────────────┘
      Distance:
        ┌─────┬─────┐
        │  0  │  20 │
        ├─────┼─────┤
        │ 20  │  0  │
        └─────┴─────┘
```

## 分配策略

### 默认策略

```rust
// 优先在本地节点分配
let local_node = numa_node_id();
let page = alloc_pages_node(local_node, order, GFP_KERNEL);
```

### 交错策略

```rust
// 轮询分配到各节点
let node = select_node(NumaPolicy::MPOL_INTERLEAVE);
let page = alloc_pages_node(node, order, GFP_KERNEL);
```

### 绑定策略

```rust
// 只在指定节点分配
let node = 1;  // 绑定到 Node 1
let page = alloc_pages_node(node, order, GFP_KERNEL);
```

## SRAT 解析

```rust
// kernel/src/drivers/acpi/srat.rs

pub struct Srat {
    pub memory_affinity: Vec<MemoryAffinity>,
    pub processor_affinity: Vec<ProcessorAffinity>,
}

pub struct MemoryAffinity {
    pub proximity_domain: u32,
    pub base_address: u64,
    pub end_address: u64,
    pub enabled: bool,
    pub hot_pluggable: bool,
}
```

## 性能考虑

| 访问类型 | 延迟 | 带宽 |
|----------|------|------|
| 本地节点 | 基准 | 基准 |
| 远程节点 | 1.5-2x | 0.8-1x |

## 使用场景

1. **多路服务器**：2/4/8 路服务器
2. **高性能计算**：HPC 工作负载
3. **数据库**：大型数据库系统

## 注意事项

1. **UMA 回退**：非 NUMA 系统自动回退到 UMA
2. **内存不均**：不同节点内存可能不同
3. **调度感知**：需要调度器配合（规划中）

## 相关文档

- [buddy - 伙伴系统](./buddy.md)
- [zone - Zone 管理](./zone.md)
- [ACPI: SRAT](../../api/drivers/acpi.md)
