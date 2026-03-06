# IOMMU

IOMMU (Input/Output Memory Management Unit) 管理设备 DMA 访问，提供地址翻译和设备隔离。

## 文件

- `kernel/src/mm/dma/mod.rs` - DMA/IOMMU façade
- `kernel/src/mm/dma/vtd.rs` - Intel VT-d 实现
- `kernel/src/mm/dma/swiotlb.rs` - SWIOTLB 实现

## IOMMU 类型

```rust
pub enum IommuType {
    None,       // 无 IOMMU
    IntelVtd,   // Intel VT-d
    AmdVi,      // AMD-Vi
    Swiotlb,    // SWIOTLB bounce buffer
}
```

## 初始化

```rust
pub fn init_iommu()
```

**检测顺序**:
1. 解析 DMAR 表
2. 初始化 Intel VT-d (如果可用)
3. 回退到 SWIOTLB (如果硬件 IOMMU 不可用)

```rust
pub fn init_iommu() {
    // 检查配置
    let mode = config::IOMMU_MODE;
    if matches!(mode, IommuMode::Off) {
        // 强制禁用，使用 SWIOTLB
        swiotlb::init(SWIOTLB_SIZE);
        return;
    }

    // 尝试初始化硬件 IOMMU
    if let Some(dmar) = acpi::get_table::<Dmar>(DMAR_SIGNATURE) {
        let info = parse_dmar(dmar);
        if matches!(mode, IommuMode::On | IommuMode::Auto) {
            if let Ok(()) = vtd::init(&info) {
                return;
            }
        }
    }

    // 回退到 SWIOTLB
    swiotlb::init(SWIOTLB_SIZE);
}
```

## Intel VT-d

### DMAR 解析

```rust
pub struct DmarInfo {
    pub host_address_width: u8,
    pub flags: DmarFlags,
    pub drhd_units: Vec<DrhdUnit>,
    pub rmrr_units: Vec<RmrrUnit>,
}
```

### DRHD 单元

```rust
pub struct DrhdUnit {
    pub register_address: u64,
    pub segment_number: u16,
    pub devices: Vec<DmarScope>,
    pub scope_count: u16,
}
```

### 初始化 VT-d

```rust
pub fn init(info: &DmarInfo) -> Result<(), IommuError>
```

**步骤**:
1. 映射寄存器
2. 设置全局命令寄存器
3. 禁用 IOMMU (传输模式)
4. 设置 fault event
5. 启用 IOMMU

实现说明（当前代码）：
- VT-d Root/Context/二级页表页由 Buddy 分配（`GFP_KERNEL_ZERO`）。
- 不再在 IOMMU 初始化阶段使用 memblock 晚期分配。

```rust
// 映射寄存器
let mmio = unsafe { &*(register_address as *const VtdRegisters) };

// 设置全局命令
mmio.global_command.write_volatile(
    GlobalCommand::new()
        .with_translation_enable(false) // Passthrough 模式
        .with_ir_translation_enable(false)
);

// 设置根表
let root_entry = vtd::create_root_entry(0);
mmio.root_table.write_volatile(root_entry);

// 启用 IOMMU
mmio.global_command.update(|cmd| {
    cmd.with_translation_enable(true)
});
```

## SWIOTLB

当硬件 IOMMU 不可用时，使用软件 bounce buffer。

实现说明（当前代码）：
- SWIOTLB 弹跳缓冲区由 Buddy 分配，使用 `GFP_DMA32` 约束在低 4GB。
- 在 Buddy 连续块受限时会降级到较小的可用连续缓冲区。

### 初始化

```rust
pub fn init(size: usize)
```

**SWIOTLB 布局**:
```
SWIOTLB Buffer (64 MB)
┌─────────────────────────────────────────────┐
│  Slot 0  │  Slot 1  │  Slot 2  │  ...       │
└─────────────────────────────────────────────┘

每个 Slot = 4KB
```

### DMA 映射

```rust
pub fn map_page(
    dev_addr: PhysAddr,
    page: PhysAddr,
    dir: DmaDirection
) -> bool
```

**映射流程**:
1. 检查地址是否可以直接访问
2. 如果不能，分配 SWIOTLB slot
3. 复制数据到 SWIOTLB
4. 返回 SWIOTLB 地址

```rust
if dev_addr < DMA_32BIT_MASK {
    // 可以直接访问
    return dev_addr;
}

// 需要 bounce buffer
let slot = alloc_swiotlb_slot()?;
bounce_buffer[slot] = page;
let swiotlb_addr = SWIOTLB_BASE + slot * 4096;

// 复制数据 (写操作)
if dir == DmaDirection::ToDevice {
    copy_page(page, bounce_buffer[slot]);
}

return swiotlb_addr;
```

### 同步

```rust
pub fn sync_single(dev_addr: DmaAddr, dir: DmaDirection)
```

```rust
let slot = addr_to_slot(dev_addr);

if dir == DmaDirection::FromDevice {
    // 从 SWIOTLB 复制回原地址
    copy_page(bounce_buffer[slot], original_addr);
}
```

## DMA API

### 一致性分配

```rust
pub fn dma_alloc_coherent(
    size: usize,
    gfp_flags: GfpFlags
) -> Option<(VirtAddr, DmaAddr)>

pub fn dma_free_coherent(
    virt: VirtAddr,
    dma: DmaAddr,
    size: usize
)
```

**特点**:
- CPU 和设备同时访问
- 物理连续
- 缓存一致

### 流式映射

```rust
pub fn dma_map_single(
    addr: VirtAddr,
    size: usize,
    dir: DmaDirection
) -> Option<DmaAddr>

pub fn dma_unmap_single(
    addr: VirtAddr,
    dma: DmaAddr,
    size: usize
)
```

**特点**:
- 非连续映射
- 需要同步操作
- 性能更好

## 使用示例

### 分配一致性内存

```rust
use kernel::mm::iommu::{dma_alloc_coherent, dma_free_coherent};

// 分配 4KB
let (virt, dma) = dma_alloc_coherent(4096, GFP_KERNEL)?;

// CPU 写入
unsafe {
    core::ptr::write_volatile(virt.as_mut_ptr::<u32>(), 0xDEADBEEF);
}

// 设备访问
device.start_dma(dma.as_u64());

// 释放
dma_free_coherent(virt, dma, 4096);
```

### 流式映射

```rust
use kernel::mm::{kmalloc, kfree};
use kernel::mm::iommu::{dma_map_single, dma_unmap_single, DmaDirection};

let buf = kmalloc(4096, GFP_KERNEL)?;
unsafe {
    core::ptr::write_bytes(buf, 0xAB, 4096);
}

// 映射给设备
let dma = dma_map_single(VirtAddr::new(buf as u64), 4096, DmaDirection::ToDevice)?;

// 同步到设备
dma_sync_single_for_device(dma, 4096);

// 设备访问...

// 同步回 CPU
dma_sync_single_for_cpu(dma, 4096);

// 解除映射
dma_unmap_single(VirtAddr::new(buf as u64), dma, 4096);
kfree(buf);
```

## 配置

```toml
# os_cfg.toml
[iommu]
mode = "auto"          # off / on / auto
translation = "passthrough"  # passthrough / translate
swiotlb_size = 67108864  # 64 MB
```

## 相关文档

- [API: iommu](../api/mm/iommu.md)
- [ACPI: DMAR](./acpi.md)
