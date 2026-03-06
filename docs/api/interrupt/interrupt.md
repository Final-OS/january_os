# interrupt - 中断子系统

`interrupt` 现在是一个 façade：顶层只暴露生命周期、通用中断控制、诊断计数与少量稳定入口；x86_64 专有的 GDT/IDT/APIC/TSC 细节统一收敛到 `kernel/src/interrupt/arch/x86_64/`。

## 当前组织

```text
kernel/src/interrupt/
├── mod.rs                 # façade：生命周期、通用控制、稳定导出
├── runtime/               # 生命周期占位
├── diag/                  # dump/stats/timer 诊断入口
├── core/                  # 核心状态占位
├── controller/            # 控制器域占位
├── trap/                  # trap 域占位
└── arch/
    ├── x86_64/
    │   ├── entry/gdt.rs
    │   ├── trap/{idt.rs,handlers.rs}
    │   ├── controller/apic.rs
    │   └── timer/{pit.rs,tsc.rs}
    ├── aarch64/           # 目录骨架占位
    └── riscv64/           # 目录骨架占位
```

## façade API

```rust
pub unsafe fn init(info: &InterruptInitInfo) -> Result<(), &'static str>;
pub unsafe fn init_bsp(info: &InterruptInitInfo) -> Result<(), &'static str>;
pub unsafe fn init_ap(
    cpu_id: usize,
    kernel_stack_top: u64,
    local_apic_addr: u64,
    direct_map_base: u64,
) -> Result<(), &'static str>;

pub fn enable_interrupts();
pub fn disable_interrupts();
pub fn interrupts_enabled() -> bool;
pub fn without_interrupts<F, R>(f: F) -> R
where
    F: FnOnce() -> R;

pub fn halt();
pub fn halt_with_interrupts();
pub fn initialized() -> bool;
pub fn timer_ticks() -> u64;
```

## x86_64 初始化类型

`InterruptInitInfo` 与 `IrqRouteOverride` 现在明确属于 x86_64 架构层：

```rust
use kernel::interrupt;
use kernel::interrupt::arch::x86_64::{InterruptInitInfo, IrqRouteOverride};

let irq_overrides = [IrqRouteOverride {
    source: 0,
    gsi: 0,
    level_triggered: false,
    active_low: false,
}; 16];

let info = InterruptInitInfo {
    kernel_stack_top: 0xFFFF800000100000,
    local_apic_addr: 0xFEE00000,
    ioapic_addr: 0xFEC00000,
    ioapic_gsi_base: 0,
    irq_override_count: 0,
    irq_overrides,
    direct_map_base: 0xFFFF880000000000,
};

unsafe { interrupt::init(&info)?; }
```

## 边界原则

- 顶层 `interrupt` 不再假定所有架构都存在 `gdt` 这类入口表实现
- GDT/IDT/APIC/PIT/TSC 等 x86_64 专有模块只放在 `arch/x86_64`
- 运行时通用调用方优先使用 façade 导出的稳定函数，而不是直接依赖目录平铺模块
- 其他架构先补齐子目录骨架，占位但不伪造实现

## 相关文档

- [gdt - GDT/TSS](./gdt.md)
- [idt - IDT](./idt.md)
- [apic - APIC](./apic.md)
- [pit - PIT](./pit.md)
