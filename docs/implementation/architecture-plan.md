# 内核整体设计与规划（2026-02-27）

本文给出 january_os 的“内核架构级”规划，重点覆盖：

1. 内核形态路线（宏内核 / 微内核 / 混合内核）
2. 子系统职责、接口与实现顺序
3. 内核全貌图与阶段里程碑

---

## 1. 内核形态路线选择

### 1.1 三种形态对比

| 方案 | 优点 | 缺点 | 适配本项目结论 |
|------|------|------|----------------|
| 宏内核（Monolithic） | 性能高、调用路径短、调试链路简单 | 可靠性隔离弱，模块崩溃影响全局 | **当前最合适**（仓库已是该形态） |
| 微内核（Microkernel） | 隔离强、容错性好、服务可重启 | IPC 成本高、早期实现复杂度高 | 作为远期研究方向 |
| 混合内核（Hybrid） | 兼顾性能与隔离，可渐进演化 | 边界设计复杂，接口治理成本高 | **推荐目标形态**（中长期） |

### 1.2 规划结论

短中期采用：
- **模块化宏内核（Monolithic-first）**
- 在不破坏性能前提下预留“服务化边界”（Hybrid-ready）

长期演化：
- 将高风险模块（文件系统、网络协议栈、部分驱动）逐步服务化
- 保留核心调度、内存管理、中断路径在内核态

---

## 2. 内核全貌图

### 2.1 逻辑分层图

```text
┌───────────────────────────────────────────────────────────────┐
│                           User Space                          │
│   libc / shell / app / daemon / tests                         │
└───────────────────────────────┬───────────────────────────────┘
                                │ syscall/abi
┌───────────────────────────────▼───────────────────────────────┐
│                       Syscall & ABI Layer                     │
│  table / dispatch / args check / errno / compat               │
└───────────────────────────────┬───────────────────────────────┘
                                │
┌───────────────────────────────▼───────────────────────────────┐
│                           Kernel Core                         │
│ task/sched | mm | vfs | ipc | net | security | time           │
└──────────────┬───────────────┬───────────────┬────────────────┘
               │               │               │
┌──────────────▼───────┐ ┌─────▼──────────┐ ┌─▼─────────────────┐
│ Interrupt/Timer/SMP   │ │ Device Model   │ │ Arch HAL (x86_64) │
│ gdt/idt/apic/ipi      │ │ bus+drivers    │ │ cpu/mmu/io/asm    │
└──────────────┬────────┘ └─────┬──────────┘ └─┬─────────────────┘
               │                │               │
┌──────────────▼────────────────▼───────────────▼────────────────┐
│                    Hardware / Firmware (UEFI)                  │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2 启动与运行主路径图

```text
UEFI
 -> Bootloader (load kernel + build BootInfo + page tables)
 -> kernel::_start
 -> init::init_kernel
 -> mm + interrupt + smp + drivers + task
 -> shell/user process
 -> syscall loop
```

---

## 3. 子系统编写规范（统一接口约束）

所有子系统应遵守以下统一接口规范：

1. 初始化接口
- `init_early()`：仅依赖最小硬件/串口
- `init_core()`：核心数据结构与全局状态
- `init_late()`：依赖其他子系统的可选能力

2. 错误模型
- 内核内部统一 `Result<T, KernelError>`
- syscall 边界统一转为 `-errno`

3. 上下文约束
- 明确每个接口的可调用上下文：
  - 中断上下文可调用
  - 可睡眠上下文可调用
  - 仅进程上下文可调用

4. 并发模型
- 自旋锁仅用于短临界区与中断上下文
- 可睡眠锁仅用于进程上下文
- RCU/无锁结构优先用于读多写少路径

5. 可观测性
- 每个子系统提供最小统计接口：
  - `stats()`
  - `dump_state()`
  - `trace_*()`（按配置开关）

---

## 4. 子系统详细规划（职责 + 接口 + 实现顺序）

## 4.1 Arch HAL（`arch/`）

职责：
- CPU、MMU、寄存器、中断门、上下文切换、特权级切换

核心接口：
```rust
pub trait ArchCpu { fn cpu_id() -> u32; fn halt() -> !; }
pub trait ArchMmu { fn switch_cr3(pml4: u64); fn flush_tlb(vaddr: u64); }
pub trait ArchIrq { fn irq_enable(); fn irq_disable(); }
```

实现顺序：
- 先固化 x86_64 trait 与实现，再引入 aarch64 空实现骨架

---

## 4.2 内存管理（`mm/`）

职责：
- 物理页分配、内核小对象分配、虚拟地址空间、用户页映射、缺页处理

核心接口：
```rust
pub fn alloc_pages(order: usize, gfp: GfpFlags) -> Option<&'static mut Page>;
pub fn map_user_page(mm: &mut MmStruct, va: u64, pa: u64, flags: PteFlags) -> Result<(), MmError>;
pub fn handle_page_fault(ctx: &FaultCtx) -> FaultResult;
```

实现重点：
- 保持 Memblock -> Buddy -> SLUB 的早期到常态切换
- 补齐 `mmap/munmap/brk` 与 fault 路径的一致性

---

## 4.3 任务与调度（`task/`）

职责：
- 进程/线程生命周期、上下文切换、运行队列、负载均衡、等待与回收

核心接口：
```rust
pub fn spawn_kernel_thread(name: &str, entry: extern "C" fn()) -> Arc<TaskRef>;
pub fn schedule() -> !;
pub fn exit_current_process(code: i32) -> !;
pub fn wait_child(target: WaitTarget, opts: WaitOpts) -> WaitResult;
```

实现重点：
- 先保证正确性（无泄漏/无僵尸异常）
- 再做公平性与多核负载均衡优化

---

## 4.4 syscall/ABI（`syscall/`）

职责：
- Linux ABI 编号兼容、参数校验、权限校验、错误码映射、调用分发

核心接口：
```rust
pub fn dispatch(args: SyscallArgs) -> usize;
pub fn register_syscall(nr: usize, handler: SyscallHandler);
```

实现重点：
- 先做“最小可运行集合”：
  - `read/write/open/close`
  - `mmap/munmap/brk`
  - `fork/clone/execve/wait4/exit`

---

## 4.5 VFS 与文件系统（`fs/`，规划中）

职责：
- inode/dentry/superblock/file 抽象
- 路径解析、挂载、文件描述符管理

核心接口：
```rust
pub trait FileOps { fn read(...); fn write(...); fn ioctl(...); }
pub trait InodeOps { fn lookup(...); fn create(...); fn unlink(...); }
pub fn vfs_open(path: &str, flags: u32, mode: u16) -> Result<Fd, FsError>;
```

实现顺序：
- 阶段 1：ramfs + fdtable + `/dev`
- 阶段 2：块设备接入 + FAT/ext 最小读能力
- 阶段 3：完整权限模型与缓存

---

## 4.6 设备模型与驱动（`drivers/`）

职责：
- bus 枚举、设备发现、驱动匹配、资源分配（BAR/IRQ/DMA）

核心接口：
```rust
pub trait Driver { fn probe(dev: &Device) -> Result<(), DriverError>; fn remove(dev: &Device); }
pub fn register_driver(bus: BusType, drv: &'static dyn Driver);
pub fn dma_map(dev: &Device, buf: &[u8]) -> DmaAddr;
```

实现顺序：
- 保持 PCI/USB/Input/TTY 稳定
- 增补块设备主线：AHCI/NVMe/virtio-blk

---

## 4.7 IPC（`task/ipc/`）

职责：
- 信号、pipe、eventfd（后续）、共享内存（后续）

核心接口：
```rust
pub fn send_signal(pid: Pid, sig: i32) -> Result<(), IpcError>;
pub fn pipe_create(flags: u32) -> Result<(Fd, Fd), IpcError>;
```

实现顺序：
- 信号语义与 wait 事件完全对齐
- pipe 先最小阻塞语义，再扩展非阻塞与 poll

---

## 4.8 网络（`net/`，规划中）

职责：
- netdev 抽象、协议栈（IPv4/TCP/UDP）、socket 层

核心接口：
```rust
pub trait NetDeviceOps { fn tx(...); fn set_rx_mode(...); }
pub fn socket(domain: i32, ty: i32, proto: i32) -> Result<Fd, NetError>;
```

实现顺序：
- 先环回 + 最小 UDP
- 再以太网驱动与 TCP

---

## 4.9 安全（`security/`，规划中）

职责：
- 权限模型、能力位（capabilities）、审计钩子

核心接口：
```rust
pub fn capable(task: &Task, cap: Capability) -> bool;
pub fn check_inode_perm(task: &Task, inode: &Inode, mask: u32) -> Result<(), SecError>;
```

实现顺序：
- 先 UID/GID + 基本 DAC
- 再 capability + LSM 风格 hook

---

## 5. 分阶段里程碑（内核工程视角）

### M1：内核基础可运行闭环（当前）
- 引导、内存、中断、SMP、任务、基础 syscall、TTY 可稳定

### M2：用户态最小可用闭环（v0.2）
- `execve + mmap + fd + wait` 全链路稳定

### M3：文件与设备闭环（v0.3）
- VFS + 块设备 + ELF 文件加载

### M4：网络与安全闭环（v0.4-v0.5）
- socket 可用，最小权限模型生效

### M5：混合内核演化（v1.0+）
- 高风险模块服务化试点（可选）

---

## 6. 关键风险与治理

1. ABI 漂移风险
- 治理：syscall 表和文档双向校验，新增 syscall 必须附测试

2. 并发一致性风险
- 治理：锁分层规则 + 中断上下文白名单 + 死锁检测日志

3. 内存安全风险
- 治理：用户指针统一校验入口，映射失败统一回滚路径

4. 架构扩展风险（x86_64 -> aarch64）
- 治理：先抽象 trait，再迁移实现，避免业务逻辑掺杂 arch 细节
