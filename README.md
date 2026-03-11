# january_os

一个以组件化宏内核方式组织的实验性操作系统内核工程，当前以 `x86_64` 为唯一可运行主线推进，并持续向 Linux ABI 兼容靠拢。`aarch64` 与 `riscv64` 目录当前保留为接口/布局骨架，而不是可启动实现。

## 仓库结构

- `boot/x86_64/`：UEFI bootloader crate（当前唯一可运行）
- `boot/aarch64/`、`boot/riscv64/`：多架构目录骨架（当前返回 `UNSUPPORTED`）
- `kernel/`：内核主体（`no_std`）
- `tools/cfg/`：读取 `os_cfg.toml` 并生成配置代码的工具
- `docs/`：VitePress 文档站点
- `target/`：构建产物

## 当前内核组织

`kernel/src/` 采用“façade + 子域目录 + 架构子树”的组织方式：

- `task/`
  - `api/`：公共类型
  - `thread/` / `proc/` / `sched/` / `ipc/`：线程、进程、调度、IPC 语义
  - `runtime/`：全局运行时状态
  - `syscall/`：task 域 ABI 入口
  - `arch/<isa>/`：架构相关上下文切换与用户态切换
- `mm/`
  - `alloc/` / `phys/` / `virt/` / `dma/`：分配、物理内存、虚拟内存、DMA/IOMMU
  - `boot/`：内存初始化编排
  - `runtime/` / `diag/` / `api/` / `syscall/`
  - `arch/<isa>/`：页表/TLB 等架构实现
- `fs/`
  - `runtime/` / `fd/` / `pipe/` / `vfs/` / `backing/` / `syscall/`
  - 顶层 `kernel/src/fs/mod.rs` 作为稳定 façade
- `interrupt/`
  - 顶层 `kernel/src/interrupt/mod.rs` 只保留生命周期、通用中断控制与稳定导出
  - `arch/x86_64/entry/`：GDT/TSS
  - `arch/x86_64/trap/`：IDT 与异常/IRQ handlers
  - `arch/x86_64/controller/`：APIC / IOAPIC
  - `arch/x86_64/timer/`：PIT / TSC
  - `arch/aarch64/`、`arch/riscv64/`：目录骨架占位
- `syscall/`
  - `arch/<isa>/`：按架构 Linux ABI 号表精确分发
  - 顶层只保留参数结构、返回值编码、号表与分发 façade
  - 共享用户态访问工具统一收敛到 `kernel/src/common/uaccess.rs`

## 当前支持矩阵

- `x86_64`：构建、启动、运行主线
- `aarch64`：目录/接口骨架，暂不可启动
- `riscv64`：目录/接口骨架，暂不可启动

## 快速开始

先安装 Rust nightly 与基础工具：

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup install nightly
rustup default nightly
rustup component add rust-src llvm-tools-preview
cargo install cargo-binutils
```

常用命令：

```bash
# 安装依赖
make install-deps

# 构建 bootloader + kernel
make build

# 运行（串口）
make run

# 运行（图形）
make run-gui

# 启动调试
make debug

# 生成 ISO
make iso

# 查看配置
make config

# 清理产物
make clean

# 查看帮助
make help
```

## 配置

项目使用 `os_cfg.toml` 作为统一配置入口，典型关注项包括：

- `arch.target`：目标架构
- `qemu.*`：QEMU 机器、内存、CPU、IOMMU 配置
- `memory.*`：页大小、zone、PCP 等内存参数
- `kernel.*`：物理基址、直接映射、初始堆、栈大小
- `iommu.*`：IOMMU 模式、翻译模式、SWIOTLB 大小
- `build.*`：优化级别、调试符号、LTO

查看展开后的实际配置：

```bash
make config
```

## 文档

开发文档位于 `docs/`，本地预览：

```bash
cd docs
pnpm install
pnpm dev
```

构建静态站点：

```bash
cd docs
pnpm build
```

## 验证建议

对 `kernel/`、`boot/`、配置或架构路径的改动，至少执行：

```bash
make build
timeout 25s make run
```

如果改动涉及文档，同步执行：

```bash
cd docs && pnpm build
```
