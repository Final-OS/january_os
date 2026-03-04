# 配置说明

january_os 使用 `os_cfg.toml` 集中管理系统配置。

### [arch] - 架构配置

```toml
[arch]
target = "x86_64"
```

| 选项 | 说明 |
|------|------|
| `target` | 目标架构，目前仅支持 `x86_64` |

---

### [qemu] - QEMU 虚拟机配置

```toml
[qemu]
memory = "256M"
smp = 4
machine = "q35"
iommu = true
```

| 选项 | 说明 | 默认值 |
|------|------|--------|
| `memory` | 内存大小，支持 M/G 后缀 | `"256M"` |
| `smp` | CPU 核心数 | `4` |
| `machine` | 机器类型（i440fx/q35） | `"q35"` |
| `iommu` | 启用 IOMMU（需要 q35） | `true` |

> `machine = "q35"` 时才能使用 IOMMU

---

### [memory] - 内存管理配置

```toml
[memory]
page_size = 4096
buddy_max_order = 11
```

| 选项 | 说明 | 默认值 |
|------|------|--------|
| `page_size` | 页大小（字节） | `4096` |
| `buddy_max_order` | Buddy 系统最大 order | `11` |

**buddy_max_order**：最大分配 = 2^max_order × page_size，`11` = 8MB

---

### [memory.zone] - 内存区域配置

```toml
[memory.zone]
dma_limit = 16777216        # 16 MB
dma32_limit = 4294967296    # 4 GB
```

| 选项 | 说明 | 默认值 |
|------|------|--------|
| `dma_limit` | ZONE_DMA 边界 | `16777216` (16MB) |
| `dma32_limit` | ZONE_DMA32 边界 | `4294967296` (4GB) |

**内存区域**：
- `ZONE_DMA`: 0 ~ dma_limit（传统 ISA DMA）
- `ZONE_DMA32`: dma_limit ~ dma32_limit（32-bit PCI DMA）
- `ZONE_NORMAL`: dma32_limit 以上（常规内存）

---

### [memory.pcp] - Per-CPU 页缓存配置

```toml
[memory.pcp]
high_watermark = 64
batch_size = 16
```

| 选项 | 说明 | 默认值 |
|------|------|--------|
| `high_watermark` | 单 CPU 缓存上限（页数） | `64` |
| `batch_size` | 批量回收/补充页数 | `16` |

---

### [kernel] - 内核配置

```toml
[kernel]
phys_base = "0x100000"
direct_map_offset = "0xFFFF880000000000"
vmalloc_start = "0xFFFFC90000000000"
vmalloc_end = "0xFFFFE8FFFFFFFFFF"
heap_init_size = 16777216   # 16 MB
stack_size = 32768          # 32 KB

[kernel.layout]
profile = "linux_full"
va_mode = "la57_prefer"
la57_fallback = "4level"
kaslr = "off"
vmemmap_start = "0xFFFFEA0000000000"
vmemmap_end = "0xFFFFEFFFFFFFFFFF"
modules_start = "0xFFFFFFFFA0000000"
modules_end = "0xFFFFFFFFFEFFFFFF"
fixmap_start = "0xFFFFFFFFFF000000"
fixmap_end = "0xFFFFFFFFFFFFF000"
manage_full_phys = true
teardown_identity_map = false
```

| 选项 | 说明 | 默认值 |
|------|------|--------|
| `phys_base` | 内核物理基址 | `"0x100000"` (1MB) |
| `direct_map_offset` | 直接映射区偏移 | `"0xFFFF880000000000"` |
| `vmalloc_start` | vmalloc 区域起始地址 | `"0xFFFFC90000000000"` |
| `vmalloc_end` | vmalloc 区域结束地址 | `"0xFFFFE8FFFFFFFFFF"` |
| `heap_init_size` | 初始堆大小 | `16777216` (16MB) |
| `stack_size` | 每栈大小 | `32768` (32KB) |

**地址布局**：直接映射 = 物理地址 + direct_map_offset，且上限需小于 vmalloc_start

| 选项 | 说明 | 默认值 |
|------|------|--------|
| `layout.profile` | 内核布局策略 | `"linux_full"` |
| `layout.va_mode` | VA 模式策略 | `"la57_prefer"` |
| `layout.la57_fallback` | LA57 回退策略 | `"4level"` |
| `layout.kaslr` | KASLR 开关 | `"off"` |
| `layout.vmemmap_start` | vmemmap 窗口起始地址 | `"0xFFFFEA0000000000"` |
| `layout.vmemmap_end` | vmemmap 窗口结束地址 | `"0xFFFFEFFFFFFFFFFF"` |
| `layout.modules_start` | modules 窗口起始地址 | `"0xFFFFFFFFA0000000"` |
| `layout.modules_end` | modules 窗口结束地址 | `"0xFFFFFFFFFEFFFFFF"` |
| `layout.fixmap_start` | fixmap 窗口起始地址 | `"0xFFFFFFFFFF000000"` |
| `layout.fixmap_end` | fixmap 窗口结束地址 | `"0xFFFFFFFFFFFFF000"` |
| `layout.manage_full_phys` | 要求完整物理内存可管理（超窗即失败） | `true` |
| `layout.teardown_identity_map` | 启动后回收 0..3GiB identity-map | `false` |

---

### [limits] - 系统限制

```toml
[limits]
max_cpus = 64
```

| 选项 | 说明 | 默认值 |
|------|------|--------|
| `max_cpus` | 最大 CPU 数量限制 | `64` |

---

### [memory_model] - 内存模型

```toml
[memory_model]
type = "uma"
```

| 选项 | 可选值 |
|------|--------|
| `type` | `"uma"` / `"numa"` |

| UMA | NUMA |
|-----|------|
| 所有 CPU 访问内存延迟相同 | 不同 CPU 访问不同节点延迟不同 |
| 单路 CPU、小型系统 | 多路服务器 |

---

### [iommu] - IOMMU 配置

```toml
[iommu]
mode = "auto"
translation = "passthrough"
swiotlb_size = 67108864     # 64 MB
```

| 选项 | 可选值 |
|------|--------|
| `mode` | `"off"` / `"on"` / `"auto"` |
| `translation` | `"passthrough"` / `"translate"` |
| `swiotlb_size` | 字节数 |

**mode**：
- `off`: 禁用 IOMMU，使用 SWIOTLB
- `on`: 强制启用 IOMMU
- `auto`: 自动检测

**translation**：
- `passthrough`: 1:1 映射，低开销
- `translate`: 完整地址翻译，更安全

---

### [debug] - 调试配置

```toml
[debug]
serial = true
mm_debug = true
page_alloc_trace = true
```

| 选项 | 说明 | 默认值 |
|------|------|--------|
| `serial` | 串口输出 | `true` |
| `mm_debug` | 内存管理详细日志 | `true` |
| `page_alloc_trace` | 页分配追踪 | `true` |

---

### [build] - 构建配置

```toml
[build]
opt_level = 3
debug_symbols = true
lto = "off"
```

| 选项 | 可选值 |
|------|--------|
| `opt_level` | `0-3`, `s`, `z` |
| `debug_symbols` | `true` / `false` |
| `lto` | `"off"` / `"thin"` / `"fat"` |

---

## 查看当前配置

```bash
make config
```

---

## 配置示例

### 开发环境
```toml
[debug]
serial = true
mm_debug = true
page_alloc_trace = true

[build]
opt_level = 0
debug_symbols = true
```

### 生产环境
```toml
[debug]
serial = false
mm_debug = false
page_alloc_trace = false

[build]
opt_level = 3
debug_symbols = false
lto = "thin"
```

### 小内存系统
```toml
[qemu]
memory = "64M"

[memory.pcp]
high_watermark = 16
batch_size = 4

[iommu]
swiotlb_size = 16777216  # 16 MB
```

### 大内存系统
```toml
[qemu]
memory = "2G"

[memory.pcp]
high_watermark = 128
batch_size = 32
```
