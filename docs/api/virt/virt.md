# virt - 虚拟化子系统

`virt` 是 january_os 的统一虚拟化入口，当前以 **Host/VMM 控制面骨架** 为主轴，同时保留 guest 环境探测与可观测性。

## 目录结构

- `kernel/src/virt/mod.rs` - 组件入口与对外 façade
- `kernel/src/virt/core/` - 能力、状态、管理器、trait 边界
- `kernel/src/virt/vm/` - VM 生命周期与控制面骨架
- `kernel/src/virt/vcpu/` - vCPU 生命周期、上下文与 exit 骨架
- `kernel/src/virt/memory/` - memslot、MMIO、地址空间与映射骨架
- `kernel/src/virt/irq/` - IRQ 注入与 route 骨架
- `kernel/src/virt/hypercall/` - hypercall ABI、dispatch、handler 骨架
- `kernel/src/virt/device/` - 虚拟设备模型与 virtio 占位
- `kernel/src/virt/service/` - syscall/control/runtime 服务入口
- `kernel/src/virt/platform/<isa>/` - `virt` 子系统专属平台后端；不复用 `virt/arch/`

## 公开接口

```rust
pub fn detect() -> VirtInfo;
pub fn create_vm() -> Result<VmId, VirtError>;
pub fn run_vcpu(vcpu: VcpuId) -> Result<(), VirtError>;
pub fn register_region(base: u64, size: u64) -> Result<(), VirtError>;
pub fn inject_irq(vector: u8) -> Result<(), VirtError>;
pub fn dispatch_syscall(args: &SyscallArgs) -> usize;
```

说明：
- `detect()` 走 `platform::<isa>::detect()`，保留当前 x86_64 CPUID hypervisor vendor 探测能力。
- `create_vm()`、`run_vcpu()`、`register_region()`、`inject_irq()` 已完成目录分层，但默认仍返回 `VirtError::Unsupported`。
- `dispatch_syscall()` 统一从 `service/syscall.rs` 进入，不再在顶层 `mod.rs` 直接承载控制面逻辑。

## 核心类型

```rust
pub struct VmId(pub u64);
pub struct VcpuId(pub u64);
pub struct MemSlotId(pub u32);
pub struct IrqRouteId(pub u32);

pub struct MmioRegion {
    pub base: u64,
    pub size: u64,
}
```

```rust
pub struct VirtState {
    pub detection_ready: bool,
    pub vm_ready: bool,
    pub vcpu_ready: bool,
    pub memory_ready: bool,
    pub irq_ready: bool,
    pub device_ready: bool,
}
```

## 平台后端规则

`virt` 子系统的架构相关实现采用 **`platform/<isa>` 特例**：

- `kernel/src/virt/platform/x86_64/`
- `kernel/src/virt/platform/aarch64/`
- `kernel/src/virt/platform/riscv64/`

原因：
- `virt` 需要表达“平台虚拟化后端”而不是泛化的系统级 `arch` 支撑。
- 同一 ISA 下后续还会继续细分 `vmx/svm/ept`、`hyp/stage2/vgic`、`h_ext/gstage/aia` 等后端能力。
- 通用 VM/VCPU/Memory/IRQ 规则仍保留在 `virt` 通用子目录中，不允许回流到平台目录。

## 当前状态

- ✅ `x86_64` hypervisor vendor 探测可用
- ✅ Host/VMM 目录层次已稳定成形
- ✅ `tests/virt/**` 已按 `detect/vm/vcpu/memory/irq/hypercall/recovery` 分组
- ⚠️ host 调用链仍为占位实现，默认返回 `Unsupported/ENOSYS`
- ⚠️ `virtio` 设备模型仅提供接口骨架，未接入真实 virtqueue/transport 运行链路

## 相关文档

- [系统设计与规划](../../implementation/architecture-plan)
- [x86_64 架构支持](../arch/x86_64)
- [开发进度总览](../../progress/overview)
