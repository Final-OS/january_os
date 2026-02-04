# 开发进度

january_os 当前开发状态与功能完成情况。

## 版本规划

| 版本 | 状态 | 说明 |
|------|------|------|
| v0.1.0 | ✅ 已完成 | 内核基础、内存管理、中断、基础驱动 |
| v0.2.0 | 🚧 开发中 | 进程管理、调度器、系统调用 |
| v0.3.0 | 📋 计划中 | 文件系统、用户空间 |
| v1.0.0 | 🎯 最终目标 | Linux ABI 兼容、生产就绪 |

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
- 16550 UART 串口 (COM1-COM4)
- Framebuffer 控制台 (VT100 转义序列)
- TTY 子系统 (串口/控制台/伪终端)

### 硬件抽象
- ACPI 表解析 (RSDP/MADT/DMAR/SRAT/FADT)
- 关机/重启支持

### 同步原语
- SpinLock / Mutex / RwLock
- Semaphore / Once / Barrier

### 构建系统
- Makefile 构建脚本
- os_cfg.toml 配置系统
- -Zbuild-std no_std 内核
- QEMU 调试支持

---

## 开发中 🚧

### 进程/任务管理
- [ ] PCB/TCB 数据结构
- [ ] 进程创建/销毁
- [ ] 线程实现
- [ ] 上下文切换

### 调度器
- [ ] 调度器框架
- [ ] EEVDF 算法
- [ ] 多核调度

### 系统调用
- [ ] syscall/sysret 指令
- [ ] 系统调用表
- [ ] 参数传递与用户/内核切换

---

## 计划中 📋

### 文件系统
- VFS 虚拟文件系统
- ext4 / FAT32 支持
- procfs / sysfs

### 用户空间
- ELF 加载器
- 动态链接器
- C 库 (musl)
- 用户程序

### 块设备
- AHCI SATA / NVMe 驱动
- virtio-blk
- 块设备层

### 网络栈
- TCP/IP 协议栈
- 以太网驱动
- Socket API

### 安全性
- 用户/内核隔离
- 权限检查
- 信号机制

---

## 技术债务

| 优先级 | 问题 |
|--------|------|
| 高 | SpinLock 死锁检测 |
| 高 | VMA/vmalloc 内存泄漏检测 |
| 中 | 错误处理统一化 |
| 中 | 中断嵌套处理改进 |
| 低 | 单元测试覆盖 |
| 低 | 性能分析与优化 |

---

## 相关文档

- [实现详解](../implementation/overview.md) - 内部实现细节
- [API 参考](../api/overview.md) - 完整 API 文档
- [配置说明](../guide/configuration.md) - os_cfg.toml 配置
