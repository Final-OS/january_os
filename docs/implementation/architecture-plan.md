# 内核整体设计与规划（2026-02-27）

本文给出 january_os 的“内核架构级”规划，重点覆盖：

1. 内核形态路线（模块化宏内核）
2. 三架构目标（x86_64 / aarch64 / riscv64）
3. 虚拟化能力目标（guest 优先，host 能力预留）
4. 子系统职责、接口与实现顺序
5. 内核全貌图与阶段里程碑

---

## 1. 内核形态路线（已冻结）

架构决策：
- **采用模块化宏内核（Modular Monolithic Kernel）**
- **采用组件化操作系统组织方式（Componentized OS）**
- **目标架构：x86_64、aarch64、riscv64（三线同构）**
- **虚拟化：先完善 guest 兼容，再分阶段补齐 host 能力**

约束边界：
- 核心组件（调度、内存、中断、syscall、设备框架）统一编译进 `kernel.bin`
- 子系统通过稳定内部接口解耦，而非通过用户态服务拆分

多架构约束：
- 通用逻辑放在 `kernel/src/<subsystem>/`，禁止掺杂架构细节
- 架构差异必须下沉到 `kernel/src/**/arch/{x86_64,aarch64,riscv64}/`
- 同一子系统对三架构保持一致接口形状，避免分叉 API

虚拟化约束：
- `virt/` 作为统一虚拟化能力入口（探测、能力描述、后续扩展）
- 先做“运行在虚拟机内”的稳定性与可观测性，再做“管理虚拟机”

---

## 2. 内核全貌图

### 2.1 内核全貌图（组件化宏内核 / `kernel.bin` 视角）

```text
User Space
  (shell / app / test)
          │
          │ syscall / fault / signal
          ▼
┌──────────────────────────────────────────────────────────────────────────┐
│                         kernel.bin (Monolithic)                          │
│                                                                          │
│  [接口入口组件]                                                          │
│  syscall::dispatch | trap/irq entry | vfs entry | device entry           │
│                                                                          │
│  [核心业务组件]                                                          │
│  task+scheduler | mm | vfs/fs | ipc | net | security                     │
│                                                                          │
│  [基础运行组件]                                                          │
│  interrupt+timer+smp | driver framework | iommu | power                  │
│                                                                          │
│  [虚拟化组件]                                                            │
│  virt::detect | hypervisor caps | pv hooks (planned)                     │
│                                                                          │
│  [公共支撑组件]                                                          │
│  sync | libs | log | config | diagnostics                                │
└───────────────────────────────┬──────────────────────────────────────────┘
                                │ stable arch interface
                                ▼
┌──────────────────────────────────────────────────────────────────────────┐
│                       Arch Backends (same shape)                         │
│   x86_64 backend | aarch64 backend | riscv64 backend                     │
└───────────────────────────────┬──────────────────────────────────────────┘
                                │
                                ▼
Hardware / Hypervisor
(Bare Metal | KVM | QEMU | VMware | Hyper-V | Xen)
```

### 2.2 整体架构图（构建 + 启动 + 运行）

```text
Source Tree
  boot/<arch> + kernel + tools/cfg + os_cfg.toml
          │
          │ make build
          ▼
Build Outputs
  boot EFI (<arch>) + kernel.bin (<arch>) + ESP layout
          │
          │ UEFI handoff (BootInfo + page tables)
          ▼
Runtime Image
  ┌────────────────────────────────────────────────────────────┐
  │  Bootloader (独立 EFI 二进制)                             │
  │  - 加载 kernel.bin                                        │
  │  - 采集硬件信息并交接                                     │
  └──────────────────────────┬─────────────────────────────────┘
                             │
                             ▼
  ┌────────────────────────────────────────────────────────────┐
  │  kernel.bin（统一内核镜像，组件化组织）                   │
  │  - init 顺序启动组件                                      │
  │  - 组件间走内核内部稳定接口                               │
  │  - arch backend 按目标架构选择                            │
  │  - virt 组件统一处理虚拟化能力探测                        │
  └──────────────────────────┬─────────────────────────────────┘
                             │
                             ▼
  User Processes + Syscall ABI
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

6. 组件初始化约束
- 核心组件应声明 `early/core/late` 阶段
- 组件初始化依赖必须显式化，禁止仅靠启动顺序隐式生效
- 当前代码基线已在 `kernel/src/init/component.rs` 引入轻量组件注册/运行器，作为启动期组件编排的统一入口

7. façade 优先
- 顶层组件应优先通过显式 façade 暴露能力，例如：
  - `drivers::init_all()`
  - `drivers::tty::* façade`
  - `fs::runtime::*`
  - `fs::backing::*`
  - `mm::component_report()`
- 顶层 `mod.rs` 可以保留兼容导出，但应避免继续扩大扁平导出面

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

当前收敛规则：
- `task/scheduler/` 负责纯运行时调度
- `task/process/` 负责进程生命周期
- `task/process/exec.rs` 承载 ELF 加载、用户栈构建、exec 地址空间替换
- `task/process/fork.rs` / `wait.rs` / `exit.rs` / `signal.rs` 承载进程生命周期 façade，`syscall` 仅保留 ABI 适配层
- `wait4` 的阻塞/观测/回收编排下沉到 `task/process/wait.rs`，`syscall/handlers/process.rs` 仅处理 PID/option 解码与用户态状态写回

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
- 处理器保持薄适配：`fork/clone/vfork/wait4/exit/kill*` 等流程统一委托给 `task` façade

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

### M5：三架构对齐 + 虚拟化增强（v1.0+）
- x86_64 / aarch64 / riscv64 组件接口对齐
- guest 虚拟化能力稳定，host 能力按阶段启用

### M6：组件化成熟（v1.0+）
- 完成组件边界稳定化与接口版本治理

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
