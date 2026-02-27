# 实现详解

本部分深入讲解 january_os 各子系统的实现细节，按系统启动流程组织。

## 启动流程

### 1. 引导阶段
```
UEFI Firmware → Bootloader → Kernel Entry
```
- [引导流程](./boot.md) - UEFI 启动、内核加载、进入高半地址空间
- [系统设计与规划](./architecture-plan.md) - 当前架构分层、设计图与阶段计划

### 2. 核心基础设施
```
GDT → IDT → APIC → 内存管理
```
- [GDT/TSS](./gdt.md) - 全局描述符表与任务状态段
- [IDT/异常处理](./idt.md) - 中断描述符表与异常分发
- [APIC](./apic.md) - 本地 APIC 与 I/O APIC
- [内存初始化](./memory-init.md) - Memblock → Buddy → SLUB 三阶段

### 3. 硬件抽象层
```
ACPI → IOMMU → 设备驱动
```
- [ACPI 解析](./acpi.md) - MADT/APIC、DMAR/IOMMU、SRAT/NUMA
- [IOMMU](./iommu.md) - Intel VT-d 硬件地址转换

### 4. 设备与子系统
- [TTY 子系统](./tty.md) - 串口、帧缓冲控制台、伪终端

## 开发工具

- [配置生成器](./cfg-tool.md) - os_cfg.toml 编译时配置处理
