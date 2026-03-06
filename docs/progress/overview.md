# 开发进度

january_os 当前开发状态与功能完成情况。

> 计划入口：[`v0.2 分批实施计划`](./v0.2-plan) | [`v0.3 分批实施计划`](./v0.3-plan)
>
> 完整路线图：[`roadmap.md`](./roadmap.md) | 技术债务追踪：[`tech-debt.md`](./tech-debt.md)

## 最近更新 🆕

**2026-03-02 - v0.2.0 正式发布**
- ✅ 全量回归测试通过（44 项 [OK]）
- ✅ 所有 Batch 1-9 验收条件满足
- ✅ 发布阻塞项 B1/B2 已完成
- ✅ QEMU 4 核 SMP 启动稳定
- 📋 v0.3 规划启动：VFS + 文件系统 + 用户空间基础

**2026-02-28 - v0.2 收口（Batch 5/6/7/8/9）**
- ✅ syscall 扩展：补齐 `pipe/pipe2` 分发与最小实现，`write` 接入普通 fd 路径（文件后端/pipe）
- ✅ IPC 最小闭环：补齐 `pipe` 读写与 `EBADF/EPIPE` 错误路径回归
- ✅ sync 演进：新增 `CondVar`，`Mutex/Semaphore` 增加可调度等待接口（非纯忙等）
- ✅ 调度器可观测：新增调度统计（local pick/steal/idle fallback）与 `test smp sched_stats`
- ✅ 文档与回归同步：更新 v0.2 执行状态与测试入口说明

**2026-02-27 - Batch 6 推进（SMP 调度安全与基础负载均衡）**
- ✅ 将 `task::Processor` 从全局单例改为 per-CPU 槽位，消除多核 `current` 覆盖风险
- ✅ 将 `IDLE_CONTEXT_SP` 改为 per-CPU 槽位，消除多核共享 idle 栈指针风险
- ✅ 调度器升级为 per-CPU runqueue，并接入基础 work-stealing 路径
- ⚠️ 调度策略仍为基础版本，后续继续推进可观测性、窃取策略与拓扑感知优化

**2026-02-27 - Batch 5 继续推进（mmap/munmap 最小匿名映射）**
- ✅ 新增 `mmap(9)/munmap(11)` syscall 最小实现并接入分发
- ✅ 支持匿名映射 VMA 建立、`munmap` 区间拆分/回收、与缺页分配链路联动
- ✅ 增加 `test mm mmap` 覆盖：正常映射、非法参数、边界与空洞回收路径
- ✅ 文件后备 `mmap` 已接入最小只读路径，支持页对齐 `offset` 与 split 后 `pgoff` 保持
- ⚠️ `MAP_SHARED` 写回、`msync`、锁页/大页语义仍待后续补齐

**2026-02-27 - Batch 3/5 最小闭环推进（exec 后端 + 文件 I/O 子集）**
- ✅ 新增最小内核文件后端（静态只读文件注册 + 按进程 fd 表）
- ✅ 接入 `open/read/close` syscall 最小实现（`O_RDONLY` 路径）
- ✅ `execve` 镜像加载改为可注册 provider，并默认接入文件后端读取
- ✅ 进程回收时自动释放 fd 表，避免僵尸/孤儿路径遗留句柄状态
- ✅ 新增 task regression 子用例，覆盖文件后端 `open/read/close/EBADF` 关键路径

**2026-02-27 - 执行链路清理（移除内核内置 demo/硬编码路径）**
- ✅ 移除 shell `runuser` 命令及其演示链路代码
- ✅ 移除 `execve` 内核内置镜像硬编码（`/init`、`/bin/demo_user`）
- ✅ `sys_execve` 调整为通过通用镜像加载后端入口获取镜像
- ✅ 同步更新 syscall 与阶段计划文档，避免行为描述失真

**2026-02-13 - Batch 3 第四阶段补强（runuser 全链路稳定性）**
- ✅ 修复 `PT_LOAD` 段数据拷贝路径：改为写入物理页直映地址，避免对 RX 用户映射写入导致卡死
- ✅ 修复 `syscall` 入口栈切换：进入 ring3 前武装内核栈，入口按 SysV 对齐后分发
- ✅ 修复 exec 映射回收计数错误，消除退出路径 `double-free detected` 告警
- ✅ 增强 `runuser/execve/syscall` 页级可观测日志，可直接定位映射与回收阶段
- ✅ `runuser` 已打通 `ring3 -> syscall(60) -> task exit -> shell` 闭环

**2026-02-13 - Batch 3 第四阶段（ring3 demo + syscall 入口）**
- ✅ 接入 x86_64 `syscall` 指令入口（`STAR/LSTAR/SFMASK/EFER.SCE` 初始化）
- ✅ 新增汇编 `syscall_entry`，打通寄存器参数到 syscall 分发
- ✅ 新增 shell 命令 `runuser`，可启动内置 `/bin/demo_user` 进入 ring3
- ✅ 新增进程退出时 exec 映射回收（含 orphan 自动回收日志）
- ⚠️ `sys_execve` 主路径仍返回 `-ENOSYS`（已具备映射链路，真实切换暂走 `runuser` 演示路径）

**2026-02-13 - Batch 3 第三阶段（PT_LOAD 真实映射与回滚）**
- ✅ 新增 `execve` PT_LOAD + 用户栈真实页映射（按段权限设置 PTE）
- ✅ 新增映射目标页冲突检测（已映射用户页返回 `-EBUSY`）
- ✅ 新增 exec 映射失败回滚（`unmap + free_page`，避免泄漏）
- ✅ 新增 `execve` 映射阶段可观测日志（mapped segment/stack pages）
- ⚠️ 当前仍返回 `-ENOSYS`（ring3 跳转尚未启用，映射在本阶段回滚）

**2026-02-13 - Batch 3 第二阶段（ELF/PT_LOAD 骨架）**
- ✅ 新增最小 ELF64 解析与 `PT_LOAD` 映射规划（exec loader skeleton）
- ✅ 新增 x86_64 用户态 `iretq` 入口帧与切换函数骨架（未实际启用）
- ✅ `sys_execve` 已接入内置镜像路径 `/init`、`/bin/demo_user`
- ✅ 增强 `execve` 诊断日志（entry/segment/page/frame）
- ⚠️ 当前仍返回 `-ENOSYS`（真实用户态映射与 ring3 切换待接入）

**2026-02-13 - Batch 3 启动（execve 第一阶段）**
- ✅ 新增 `execve(59)` syscall 分发与处理入口
- ✅ 增加 `pathname/argv/envp` 用户指针与边界校验（`EFAULT/E2BIG/ENAMETOOLONG/ENOENT`）
- ✅ 增加进程 exec 请求可观测状态（path/argc/envc/seq）
- ⚠️ 当前仍返回 `-ENOSYS`（用户态 ELF 加载与 ring3 切换待接入）

**2026-02-13 - 任务管理阶段推进（Batch 2/4）**
- ✅ 新增进程创建 syscall：`clone/fork/vfork`（最小语义）
- ✅ `fork` 已补齐用户态返回点复制与私有页真实 COW；`vfork` 保持共享地址空间路径
- ✅ 新增进程组 syscall：`getpgid/getpgrp/setpgid/setsid`（最小语义）
- ✅ 新增信号 syscall：`kill/tkill/tgkill`（支持 `SIGCHLD/SIGTERM/SIGKILL/SIGSTOP/SIGCONT`）
- ✅ `wait4` 与 `SIGSTOP/SIGCONT/SIGKILL` 事件链路联动
- ✅ 补充任务回收/信号路径可观测日志

**2026-03-06 - 组件化宏内核收敛（初始化编排 + façade）**
- ✅ 启动路径新增轻量组件注册/运行器，显式标注 `early/core/late` 阶段与依赖
- ✅ `drivers` 增加 `init_all()` façade，避免启动器逐个耦合内部驱动模块
- ✅ `fs` 增加 `init_runtime()` 报告接口，显式暴露 rootfs/initramfs 初始化结果
- ✅ `fs` 新增 `runtime` / `backing` façade，开始把进程文件运行时与 mmap backing 适配边界显式化
- ✅ `mm/mod.rs` 从通配 re-export 收紧为显式导出 + 兼容命名空间模块
- ✅ `task/exec.rs` 已迁入 `task/process/exec.rs`，明确归属进程生命周期组件
- ✅ `fork/wait/exit` 已开始通过 `task/process/*` façade 暴露，`syscall/process` 向薄适配层收敛
- ⚠️ 组件化目前主要落在启动编排与接口 façade，跨子系统深层直接依赖仍需继续收敛

**2026-02-08 - 任务管理与系统调用实现**
- ✅ 任务管理基础（PCB/TCB、内核栈、任务状态）
- ✅ 上下文切换（x86_64 汇编实现）
- ✅ Round-Robin 调度器
- ✅ SMP 多核支持（AP 核心启动、Per-CPU 数据）
- ✅ 系统调用框架（syscall/sysret、系统调用表）
- ✅ 基础系统调用（getpid、exit、wait4 等）
- ✅ LRU/RCU 完整测试覆盖（100% API 覆盖）

**2026-02-08 - 数据结构优化完成**
- ✅ 实现通用 B-Tree（分支因子 16，缓存优化）
- ✅ Maple Tree 从红黑树迁移到 B-Tree
- ✅ LRU 缓存重写（O(1) 操作，双向链表实现）
- ✅ RCU 升级（宽限期管理、延迟回收、call_rcu）
- ✅ Radix Tree 修复（间隙查找、迭代器）
- ✅ 完整的测试覆盖（所有数据结构通过测试）

---

## 版本规划

> 详细路线图：[`roadmap.md`](./roadmap.md)

| 版本 | 状态 | 核心目标 | 目标日期 |
|------|------|----------|----------|
| v0.1.0 | ✅ 已完成 | 内核基础、内存管理、中断、基础驱动 | 2026-Q1 |
| v0.2.0 | ✅ 已完成 | 进程管理、调度器、系统调用、IPC/Sync | 2026-Q2 |
| v0.3.0 | 🚧 进行中 | 块设备、VFS、FAT32/ext4、用户空间基础 | 2026-Q3 |
| v0.4.0 | 📋 计划中 | 可写文件系统、syscall 150+、initramfs | 2026-Q4 |
| v0.5.0 | 📋 计划中 | 网络栈基础、virtio-net、Socket API | 2027-Q1 |
| v0.6.0 | 📋 计划中 | 网络栈完善、epoll、syscall 200+ | 2027-Q2 |
| v0.7.0 | 📋 计划中 | **aarch64 移植** | 2027-Q3 |
| v0.8.0 | 📋 计划中 | **riscv64 移植** | 2027-Q4 |
| v0.9.0 | 📋 计划中 | 安全加固、性能优化、syscall 250+ | 2028-Q1 |
| v1.0.0 | 🎯 目标 | **正式发布**：三架构 + Linux ABI 兼容 + 生产就绪 | 2028-Q2 |

---

## 已完成 ✅

### 引导与内核基础
- UEFI 引导程序 (uefi-rs)
- GOP 图形输出支持
- Higher-half 内核映射
- 直接映射区
- BootInfo 结构体传递

### 内存管理
- Memblock → Buddy → SLUB 三阶段分配
- Zone 管理 (DMA/DMA32/Normal)
- VMA 虚拟内存区域
- vmalloc/vfree
- PCP Per-CPU 页缓存
- NUMA 多节点支持
- IOMMU (Intel VT-d + SWIOTLB)
- 页错误处理 (COW、栈增长)

### 中断处理
- GDT/TSS/IST
- IDT 与异常处理
- Local APIC / I/O APIC
- APIC Timer (PIT 校准)
- IRQ 处理框架

### 设备驱动
- PS/2 键盘驱动
- USB xHCI 控制器 (支持输入设备)
- USB HID 键盘/鼠标 (Boot Protocol)
- 16550 UART 串口 (COM1-COM4)
- Framebuffer 控制台 (VT100 转义序列)
- TTY 子系统 (串口/控制台/伪终端)

### 硬件抽象
- ACPI 表解析 (RSDP/MADT/DMAR/SRAT/FADT)
- 关机/重启支持

### 同步原语
- SpinLock / Mutex / RwLock
- Semaphore / Once / Barrier / CondVar
- RCU (Read-Copy-Update) - 无锁读取同步

### SMP 多核支持
- **AP 核心启动** - ACPI MADT 解析，INIT-SIPI-SIPI 序列
- **Per-CPU 数据** - CPU ID 分配，Per-CPU 变量
- **核间通信** - APIC IPI 支持
- **多核调度** - 支持多核任务调度（基础实现）

### 内核数据结构
- **红黑树 (RbTree)** - 有序键值对存储，O(log n) 操作
- **LRU 缓存** - 双向链表 + HashMap，O(1) 所有操作
  - 支持延迟回收和多版本并发
  - 完整的迭代器支持
- **RCU (Read-Copy-Update)** - 无锁读取同步机制
  - 宽限期管理和延迟回收
  - call_rcu 和 rcu_barrier 支持
  - RCU 指针类型和辅助宏
- **Radix Tree** - 64-way 多级基数树
  - O(log₆₄ N) 查找性能
  - 完整的范围操作和迭代器
- **B-Tree** - 通用 B-Tree 实现
  - 分支因子 16，缓存友好
  - 支持插入、删除、范围查询
  - 正确的内部节点键值处理
- **Maple Tree** - 区间树（基于 B-Tree）
  - VMA 管理的核心数据结构
  - 支持区间查询和间隙搜索
  - 从红黑树迁移到 B-Tree 实现

### 构建系统
- Makefile 构建脚本
- os_cfg.toml 配置系统
- -Zbuild-std no_std 内核
- QEMU 调试支持

### 任务管理
- **Task 结构** - PCB/TCB、内核栈、任务状态
- **上下文切换** - x86_64 汇编实现（__switch）
- **内核线程** - spawn_kernel_thread 支持
- **任务 ID 管理** - TaskId/ProcessId 分配
- **任务生命周期** - 创建、运行、退出

### 调度器
- **Round-Robin 调度器** - 基础时间片轮转
- **Per-CPU 就绪队列** - 本地队列 + 基础 work-stealing
- **调度框架** - schedule() 函数
- **空闲上下文** - per-CPU idle context 切换

### 系统调用
- **syscall/sysret 机制** - x86_64 Long Mode 系统调用
- **系统调用表** - Linux ABI 兼容（300+ 系统调用定义）
- **参数传递** - SyscallArgs 结构体
- **已实现的系统调用**:
  - read/open/close - 最小只读文件 I/O（静态后端）
  - mmap/munmap - 匿名 + file-backed 最小路径（静态后端）
  - pipe/pipe2 - 最小管道语义（读写/关闭/错误路径）
  - getpid/getppid/gettid - 进程/线程 ID 查询
  - exit/exit_group - 进程退出
  - execve - ELF 加载、PT_LOAD 映射、ring3 切换（最小镜像后端）
  - wait4 - 等待子进程（支持 PID/PGRP 过滤、WNOHANG/WUNTRACED/WCONTINUED、__WNOTHREAD/__WCLONE/__WALL、rusage 运行时统计）
  - write - 控制台输出 + pipe/fd 写路径

---

## 开发中 🚧

### IPC 进程间通信
- [x] 信号机制（Signal，最小闭环）
- [x] 管道（Pipe，最小闭环）
- [ ] 共享内存
- [ ] 消息队列

### 调度器增强
- [ ] EEVDF 算法实现（目标版本：`v0.4.0`）
- [ ] 实时调度策略
- [ ] 负载均衡（当前已具备基础 work-stealing，待策略优化）
- [x] 调度可观测性（local pick/steal/idle fallback 统计）
- [ ] CPU 亲和性与拓扑感知（NUMA/缓存局部性）

### 系统调用扩展
- [x] fork/clone - 进程创建（最小语义）
- [x] execve - 程序执行（最小闭环）
- [ ] 文件 I/O 系统调用（已完成 `open/read/close` 子集）
- [ ] 内存管理系统调用（已完成匿名 `mmap/munmap` 子集）
- [ ] 网络系统调用（socket/bind/listen）
- [ ] syscall 全量完备与 Linux 兼容收口（目标版本：`v1.0.0`）

---

## 计划中 📋

### 文件系统 (v0.3.0)
- VFS 虚拟文件系统
- ext4 / FAT32 支持
- procfs / sysfs
- 文件描述符管理
- 目录操作

### 用户空间基础 (v0.3.0)
- **ELF 加载器** - 加载和执行 ELF 二进制文件
- **动态链接器** - 共享库支持 (.so)
- **C 库移植** - musl libc 或 glibc 子集
- **用户程序** - 基础 shell 和工具
- **用户态地址空间** - 独立的页表和内存隔离
- **用户态系统调用** - 完整的系统调用支持

### 块设备 (v0.3.x)
- virtio-blk（`v0.3.0` 主链路）
- virtio-scsi（`v0.3.1` 最小链路）
- VMware PVSCSI（`v0.3.2` 最小链路）
- AHCI SATA / NVMe（`v0.3.2` 最小链路，`v0.4+` 增强）
- 块设备层
- 缓存管理

### 虚拟化支持 (v0.4.0)
- **KVM 兼容层** - 支持运行虚拟机
- **Intel VT-x / AMD-V** - 硬件虚拟化支持
- **EPT/NPT** - 扩展页表支持
- **VMCS/VMCB 管理** - 虚拟机控制结构
- **虚拟设备** - virtio 设备模拟
- **虚拟机监控器** - 基础 hypervisor 功能
- **嵌套虚拟化** - 支持在虚拟机中运行虚拟机

### 图形栈 (v0.5.0)
- **DRM/KMS** - Direct Rendering Manager
- **GPU 驱动** - Intel/AMD/NVIDIA 基础支持
- **2D 加速** - 基础图形加速
- **3D 加速** - OpenGL/Vulkan 支持
- **显示管理** - 多显示器支持
- **合成器** - Wayland 合成器

### 桌面系统 (v0.5.0)
- **Wayland 协议** - 现代显示服务器协议
- **窗口管理器** - 基础窗口管理
- **桌面环境** - 简单的桌面环境
- **GUI 工具包** - 图形界面库
- **输入法框架** - 中文输入支持
- **桌面应用** - 终端、文件管理器、浏览器

### 网络栈 (v0.6.0)
- TCP/IP 协议栈
- 以太网驱动
- Socket API
- 网络工具

### 安全性 (v0.7.0)
- 用户/内核隔离
- 权限检查
- 信号机制
- SELinux/AppArmor 支持

---

## 数据结构优化计划 🔧

### LRU 缓存优化
- [ ] **Per-CPU LRU 列表** - 减少锁竞争，提升多核性能
- [ ] **分层 LRU** - 活跃/非活跃列表分离（类似 Linux）
- [ ] **批量操作** - 批量淘汰和批量插入
- [ ] **自适应容量** - 根据内存压力动态调整
- [ ] **统计信息** - 命中率、淘汰率等性能指标

### RCU 优化
- [ ] **Per-CPU 读者计数** - 消除缓存行竞争，实现真正的零开销读取
- [ ] **异步回调队列** - 后台线程处理延迟回收
- [ ] **批量回收** - 减少锁竞争，提升回收效率
- [ ] **层次化 RCU** - 支持大规模并发（TREE RCU）
- [ ] **宽限期优化** - 快速路径检测和自适应等待
- [ ] **内存屏障优化** - 针对不同架构的优化

### B-Tree 优化
- [ ] **节点预分配** - 减少分配开销
- [ ] **自适应分支因子** - 根据数据特征调整
- [ ] **批量插入/删除** - 优化批量操作性能
- [ ] **范围查询优化** - 迭代器性能提升
- [ ] **压缩节点** - 减少内存占用

### Radix Tree 优化
- [ ] **自适应高度** - 根据键分布动态调整
- [ ] **路径压缩** - 减少树高度
- [ ] **SIMD 加速** - 使用 SIMD 指令加速查找
- [ ] **预取优化** - 减少缓存未命中

### Maple Tree 优化
- [ ] **RCU 保护** - 支持无锁读取
- [ ] **范围锁** - 细粒度锁定
- [ ] **批量操作** - 批量插入/删除/查询
- [ ] **内存池** - 减少分配开销

### 通用优化
- [ ] **内存池管理** - 为数据结构提供专用内存池
- [ ] **NUMA 感知** - 优化 NUMA 系统性能
- [ ] **性能基准测试** - 建立完整的性能测试框架
- [ ] **压力测试** - 多核并发压力测试
- [ ] **内存泄漏检测** - 自动化内存泄漏检测

---

## 技术债务

| 优先级 | 问题 |
|--------|------|
| 中 | 中断嵌套处理改进 |
| 低 | 性能分析与优化 |

---

## 相关文档

- [实现详解](../implementation/overview.md) - 内部实现细节
- [API 参考](../api/overview.md) - 完整 API 文档
- [配置说明](../guide/configuration.md) - os_cfg.toml 配置
