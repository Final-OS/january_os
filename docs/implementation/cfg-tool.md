# 配置生成器

配置生成器 (`tools/cfg`) 解析 `os_cfg.toml` 并生成 Rust 配置代码。

## 文件

- `tools/cfg/src/main.rs` - 配置工具主程序
- `tools/cfg/Cargo.toml` - 配置工具构建配置
- `os_cfg.toml` - 系统配置文件

## 功能

配置工具提供三个命令：

### 1. get - 读取配置值

```bash
$(CFG) get <key>
```

**示例**:
```bash
$ $(CFG) get qemu.memory
256M

$ $(CFG) get kernel.phys_base
0x100000

$ $(CFG) get memory.pcp.high_watermark
64
```

### 2. generate - 生成 Rust 代码

```bash
$(CFG) generate <output_file>
```

**输出示例**:
```rust
// 自动生成 - 不要手动编辑
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(dead_code)]
#![warn(unused)]

pub const ARCH_TARGET: &str = "x86_64";
pub const QEMU_MEMORY_MB: u64 = 256;
pub const QEMU_SMP: u32 = 4;
pub const PAGE_SIZE: u64 = 4096;
pub const KERNEL_PHYS_BASE: u64 = 0x100000;
pub const DIRECT_MAP_OFFSET: u64 = 0xFFFF880000000000;
// ...
```

### 3. show - 显示配置

```bash
$ $(CFG) show
=== january_os configuration ===
arch.target = x86_64
qemu.memory = 256M
qemu.smp = 4
...
```

## Makefile 集成

```makefile
# 确保 cfg 工具已构建
$(CFG): tools/cfg/src/main.rs tools/cfg/Cargo.toml
	@echo "==> Building cfg tool..."
	@mkdir -p $(TOOLS_BIN)
	@cd tools/cfg && CARGO_TARGET_DIR=/tmp/january_os_tools cargo build --release -q
	@cp /tmp/january_os_tools/release/cfg $(CFG)

# 从配置文件读取
ARCH = $(shell $(CFG) get arch.target)
QEMU_MEMORY = $(shell $(CFG) get qemu.memory)
```

## 配置文件格式

```toml
# os_cfg.toml

[arch]
target = "x86_64"

[qemu]
memory = "256M"
smp = 4

[memory]
page_size = 4096
buddy_max_order = 11

[kernel]
phys_base = "0x100000"
direct_map_offset = "0xFFFF880000000000"

[iommu]
mode = "auto"
translation = "passthrough"
```

## 代码生成

### 类型映射

| TOML 类型 | Rust 类型 |
|-----------|-----------|
| 字符串 | `&str` |
| 整数 | `u64`, `u32`, 等 |
| 布尔 | `bool` |
| 数组 | `[T; N]` |

### 复杂类型处理

```rust
// 枚举
enum IommuMode {
    Off = "off",
    On = "on",
    Auto = "auto",
}

impl From<&str> for IommuMode {
    fn from(s: &str) -> Self {
        match s {
            "off" => IommuMode::Off,
            "on" => IommuMode::On,
            "auto" => IommuMode::Auto,
            _ => panic!("Invalid IOMMU mode: {}", s),
        }
    }
}

// 生成
let mode: IommuMode = table.get("iommu", "mode")?.into();
```

## 使用流程

```
1. 用户编辑 os_cfg.toml
    │
    ▼
2. make build
    │
    ▼
3. Makefile 调用 $(CFG) generate
    │
    ▼
4. 生成 kernel/src/generated/config.rs
    │
    ▼
5. 内核编译时使用生成的常量
```

## 相关文档

- [配置说明](../guide/configuration.md)
- [引导流程](./boot.md)
