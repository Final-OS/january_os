# SMP (Symmetric Multi-Processing) API

SMP 子系统提供多核处理器支持，包括 AP 核心启动、Per-CPU 数据管理和核间通信。

---

## 初始化

### smp::init

初始化 SMP 子系统，启动所有 AP 核心。

```rust
pub fn init(direct_map_base: u64, expected_cpus: usize)
```

**参数**:
- `direct_map_base`: 直接映射区基地址
- `expected_cpus`: 期望启动的 CPU 总数（包括 BSP）

**说明**:
- 解析 ACPI MADT 表
- 准备 Trampoline 代码
- 发送 INIT-SIPI-SIPI 序列
- 等待 AP 核心启动

**示例**:
```rust
use crate::smp;

// 启动 4 个 CPU 核心
smp::init(DIRECT_MAP_BASE, 4);
```

---

## CPU 管理

### alloc_cpu_id

分配新的 CPU ID。

```rust
pub(crate) fn alloc_cpu_id() -> usize
```

**返回**: `usize` - 新分配的 CPU ID

**说明**:
- BSP (Bootstrap Processor) 的 ID 为 0
- AP (Application Processor) 的 ID 从 1 开始递增
- 使用原子操作保证线程安全

**注意**: 这是内部函数，通常不直接调用。

---

## 架构相关 API (x86_64)

### arch::prepare_smp

准备 SMP 环境。

```rust
pub unsafe fn prepare_smp(madt: &Madt, direct_map_base: u64)
```

**参数**:
- `madt`: ACPI MADT 表
- `direct_map_base`: 直接映射区基地址

**说明**:
- 设置 Trampoline 代码
- 配置 ACPI Wakeup 向量
- 准备 AP 启动环境

### arch::boot_ap

启动单个 AP 核心。

```rust
pub fn boot_ap(apic_id: u32, direct_map_base: u64)
```

**参数**:
- `apic_id`: AP 的 Local APIC ID
- `direct_map_base`: 直接映射区基地址

**说明**:
- 发送 INIT IPI
- 等待 10ms
- 发送 SIPI IPI（两次）
- 等待 AP 启动

---

## Per-CPU 数据

### CPU ID 获取

获取当前 CPU 的 ID。

```rust
// 通过 Local APIC 获取
use crate::interrupt::local_apic_id;

let cpu_id = local_apic_id();
```

### Per-CPU 变量

当前实现的 Per-CPU 数据：

1. **当前任务** (`PROCESSOR`)
   ```rust
   use crate::task::processor::PROCESSOR;

   let current = PROCESSOR.lock().current();
   ```

2. **Per-CPU 页缓存** (PCP)
   ```rust
   use crate::mm::page::pcp;

   // PCP 会自动使用当前 CPU 的缓存
   ```

---

## 核间通信 (IPI)

### APIC IPI

通过 Local APIC 发送核间中断。

```rust
use crate::interrupt::arch::x86_64::controller::apic;

// 发送 IPI 到指定 CPU
apic::send_ipi(target_apic_id, vector);

// 广播 IPI 到所有 CPU
apic::send_ipi_all(vector);

// 发送 IPI 到除自己外的所有 CPU
apic::send_ipi_all_except_self(vector);
```

**说明**:
- `target_apic_id`: 目标 CPU 的 Local APIC ID
- `vector`: 中断向量号

---

## ACPI MADT 解析

### Madt 结构

ACPI Multiple APIC Description Table。

```rust
pub struct Madt {
    // MADT 表数据
}

impl Madt {
    pub fn entries(&self) -> MadtIterator;
}
```

### MadtEntry

MADT 表项枚举。

```rust
pub enum MadtEntry {
    LocalApic(LocalApicEntry),
    IoApic(IoApicEntry),
    InterruptOverride(InterruptOverrideEntry),
    // ...
}
```

### LocalApicEntry

Local APIC 表项。

```rust
pub struct LocalApicEntry {
    pub processor_id: u8,
    pub apic_id: u8,
    pub flags: u32,
}

impl LocalApicEntry {
    pub fn is_enabled(&self) -> bool;
    pub fn is_online_capable(&self) -> bool;
}
```

---

## 使用示例

### 启动多核系统

```rust
use crate::smp;
use crate::drivers::acpi;

// 从配置获取 CPU 数量
let cpu_count = 4;

// 初始化 SMP
smp::init(DIRECT_MAP_BASE, cpu_count);

// 此时所有 CPU 核心都已启动
println!("All {} CPUs are online", cpu_count);
```

### 获取当前 CPU ID

```rust
use crate::interrupt::local_apic_id;

let cpu_id = local_apic_id();
println!("Running on CPU {}", cpu_id);
```

### Per-CPU 任务管理

```rust
use crate::task::processor::PROCESSOR;

// 获取当前 CPU 的当前任务
if let Some(task) = PROCESSOR.lock().current() {
    let t = task.lock();
    println!("CPU {} running task: {}", local_apic_id(), t.name);
}
```

### 发送核间中断

```rust
use crate::interrupt::arch::x86_64::controller::apic;

// 发送 IPI 到 CPU 1
apic::send_ipi(1, 0x30); // 向量 0x30

// 广播 IPI 到所有 CPU
apic::send_ipi_all(0x31);
```

---

## 启动流程

### BSP (Bootstrap Processor) 启动流程

1. UEFI 引导程序加载内核
2. 内核初始化（BSP 上运行）
3. 初始化内存管理
4. 初始化中断处理
5. 调用 `smp::init()` 启动 AP 核心
6. 继续内核初始化

### AP (Application Processor) 启动流程

1. BSP 发送 INIT IPI
2. AP 进入等待状态
3. BSP 发送 SIPI IPI（包含 Trampoline 代码地址）
4. AP 从 Trampoline 代码开始执行
5. AP 切换到保护模式
6. AP 切换到长模式
7. AP 跳转到内核代码
8. AP 初始化自己的 GDT/IDT/APIC
9. AP 分配 CPU ID
10. AP 进入调度循环

---

## 同步机制

### CPU 启动同步

使用原子计数器同步 CPU 启动：

```rust
static NEXT_CPU_ID: AtomicUsize = AtomicUsize::new(1);

// BSP 等待所有 AP 启动
while NEXT_CPU_ID.load(Ordering::SeqCst) < expected_cpus {
    core::hint::spin_loop();
}
```

### Per-CPU 数据同步

Per-CPU 数据通常不需要同步，因为每个 CPU 访问自己的数据。但如果需要跨 CPU 访问，需要使用锁或原子操作。

---

## 性能考虑

### 缓存一致性

- **问题**: 多核系统中，每个 CPU 有自己的缓存，需要保持一致性
- **解决**: 使用 MESI 协议（硬件自动处理）
- **优化**: 使用 Per-CPU 数据减少缓存行竞争

### 锁竞争

- **问题**: 全局锁会导致多核性能下降
- **解决**: 使用 Per-CPU 数据结构
- **示例**: Per-CPU 调度队列、Per-CPU 页缓存

### NUMA 优化

- **问题**: 跨 NUMA 节点访问内存延迟高
- **解决**: 优先分配本地节点内存
- **API**: 参见 [NUMA API](../mm/numa.md)

---

## 限制和注意事项

1. **当前限制**
   - 调度器使用全局锁（性能瓶颈）
   - 没有 Per-CPU 调度队列
   - 没有负载均衡

2. **未来改进**
   - 实现 Per-CPU 调度队列
   - 添加负载均衡算法
   - 实现 CPU 亲和性
   - 支持 CPU 热插拔

3. **调试建议**
   - 使用 `local_apic_id()` 跟踪代码在哪个 CPU 上运行
   - 注意死锁问题（多核环境更容易出现）
   - 使用原子操作保证线程安全

---

## 相关 API

- [Interrupt API](../interrupt/apic.md) - APIC 和 IPI
- [Task API](../task/task.md) - 任务管理
- [NUMA API](../mm/numa.md) - NUMA 支持
- [ACPI API](../drivers/acpi.md) - ACPI 表解析

---

**最后更新**: 2026-02-08
