# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

january_os is an operating system written in Rust for x86_64 with UEFI boot, targeting Linux ABI compatibility.

## Project Structure

```
january_os/
├── boot/x86_64/           # UEFI bootloader (workspace member)
│   └── src/main.rs        # uefi-rs bootloader with GOP
├── kernel/                # Standalone kernel crate (no_std)
│   ├── src/
│   │   ├── main.rs        # Kernel entry point
│   │   ├── lib.rs         # Kernel library root
│   │   ├── generated/     # Auto-generated config from os_cfg.toml
│   │   ├── arch/          # Architecture-specific kernel code
│   │   ├── drivers/       # Device drivers
│   │   ├── interrupt/     # GDT, IDT, handlers, APIC
│   │   ├── mm/            # Memory management
│   │   └── sync/          # Synchronization primitives
│   ├── arch/x86_64/       # Arch-specific kernel files
│   │   └── linker.ld      # Kernel linker script
│   └── Cargo.toml         # Kernel crate (standalone workspace)
├── tools/cfg/             # Config code generator
│   └── src/main.rs        # Parses os_cfg.toml, generates Rust
├── docs/                  # Project documentation (in Chinese)
├── os_cfg.toml            # System configuration
├── Makefile               # Build system
└── target/                # Build output
```

**Workspace layout**: The bootloader (`boot/x86_64/`) is a Cargo workspace member, while the kernel (`kernel/`) is a standalone workspace that uses `-Zbuild-std` for `no_std` support. The kernel is built separately from the workspace.

## Prerequisites

```bash
# Rust toolchain
rustup install nightly
rustup default nightly
rustup component add rust-src llvm-tools-preview
rustup target add x86_64-unknown-uefi
cargo install cargo-binutils

# QEMU and OVMF
# Ubuntu/Debian: sudo apt install qemu-system-x86 ovmf mtools xorriso
# Fedora:        sudo dnf install qemu-system-x86 edk2-ovmf mtools xorriso
# Arch:          sudo pacman -S qemu-full edk2-ovmf mtools xorriso

# Or run: make install-deps
```

## Common Commands

```bash
make build              # Build bootloader and kernel
make build-tools        # Build config generator tool only
make build-boot         # Build only bootloader
make build-kernel       # Build only kernel
make run                # Run in QEMU (serial console, Ctrl+A X to exit)
make debug              # Run with GDB server on port 1234
make iso                # Create bootable ISO (for VMware/real hardware)
make clean              # Clean build artifacts
make config             # Show current configuration from os_cfg.toml
make help               # Show all targets
```

**QEMU exit**: Press `Ctrl+A X` to quit QEMU.

**Debugging**: Use `make debug` then connect with GDB:
```bash
gdb -ex "target remote :1234" -ex "symbol-file target/x86_64-unknown-none/release/january_os-kernel"
```

## Boot Process

1. **UEFI Firmware** loads `BOOTX64.EFI` from EFI System Partition
2. **Bootloader** (`boot/x86_64/`):
   - Sets up graphics mode (framebuffer) via GOP
   - Loads kernel from `/EFI/january_os/kernel.bin`
   - Exits UEFI boot services
   - Jumps to kernel entry point
3. **Kernel** (`kernel/`):
   - Sets up GDT, IDT, paging
   - Initializes memory management (memblock → buddy → slub)
   - Brings up APIC, ACPI, drivers
   - Enters interactive shell

## Build Process

The build system uses a custom config generator tool (`tools/cfg`):

1. The `tools/cfg` tool parses `os_cfg.toml` (TOML → Rust code generation)
2. Generates Rust config files in `kernel/src/generated/`
3. Builds UEFI bootloader for `x86_64-unknown-uefi` (workspace member)
4. Builds kernel with `-Zbuild-std=core,alloc` (standalone, no_std)
5. Uses `rust-objcopy` to convert ELF to binary
6. Creates EFI System Partition (ESP) with bootloader and kernel

**Important**: `kernel/src/generated/config.rs` is auto-generated - do not edit manually.

**Config tool (`tools/cfg`) usage**:
- `$(CFG) get <key>` - Read a config value
- `$(CFG) generate <output>` - Generate Rust config file
- `$(CFG) show` - Display current configuration

## Kernel Build Notes

The kernel uses Rust's `-Zbuild-std` feature to build `core` and `alloc` libraries with custom options for `no_std` environment. This allows use of `Vec`, `Box`, `HashMap`, etc. without standard library.

Key features enabled in `kernel/src/lib.rs`:
- `#![no_std]` - No standard library
- `#![feature(alloc_error_handler)]` - Custom allocation error handler
- `#![feature(abi_x86_interrupt)]` - x86 interrupt ABI

**Important**: When adding dependencies to the kernel, ensure they are `no_std` compatible.

## Architecture

### Memory Management (kernel/src/mm/)

Memory initialization follows a progression: **memblock → buddy → slub**

| Component | File | Description |
|-----------|------|-------------|
| Memblock | memblock.rs | Early boot allocator (before paging) |
| struct page | page.rs | Page frame descriptor (flags, refcount, order) |
| Zone | zone.rs | DMA / DMA32 / Normal zones with watermarks |
| Buddy System | buddy.rs | alloc_pages / free_pages (2^order pages) |
| PCP | pcp.rs | Per-CPU Page Cache for hot pages |
| SLUB | slub.rs | kmalloc / kfree / kzalloc (8B-8KB) |
| VMA | vma.rs | Virtual memory areas, mmap support |
| vmalloc | vmalloc.rs | Virtual contiguous allocation, ioremap |
| Page Fault | fault.rs | Demand paging, COW, stack growth |
| NUMA | numa.rs | Multi-node memory support (SRAT-based) |
| IOMMU | iommu/ | Intel VT-d, AMD-Vi, SWIOTLB |

**Architecture-specific MM** (`mm/arch/x86_64/`):
- `paging.rs` - Page table manipulation (PML4, PDPD, PD, PT)
- `tlb.rs` - TLB invalidation (invept, invvpid, invpcid)

### Drivers (kernel/src/drivers/)

| Subsystem | Path | Components |
|-----------|------|------------|
| ACPI | drivers/acpi/ | RSDP, XSDT, MADT, DMAR, SRAT tables |
| Input | drivers/input/ps2/ | PS/2 keyboard driver |
| Input | drivers/input/hid/ | USB HID framework (keyboard, mouse) |
| TTY | drivers/tty/serial/ | 16550 UART (COM1-COM4) |
| TTY | drivers/tty/console/ | Framebuffer console, VT100 parser, PSF font |
| TTY | drivers/tty/pty/ | Pseudo-terminal (master/slave) |
| TTY | drivers/tty/fbcon.rs | Framebuffer console driver |

### Architecture-Specific Code (kernel/src/arch/x86_64/)

- `mod.rs` - Architecture initialization
- `serial.rs` - 16550 UART driver for early debug output

### Interrupt Handling (kernel/src/interrupt/)

- GDT with TSS (kernel/user segments, IST)
- IDT with exception/IRQ handlers
- Local APIC / I/O APIC
- APIC Timer (calibrated via PIT)
- PS/2 keyboard interrupt (IRQ1)
- Serial port interrupt (IRQ4)
- PIT (8254 timer) for calibration

### Synchronization Primitives (kernel/src/sync/)

- Spinlock - Simple spin-based mutual exclusion
- RwLock - Read-write lock with reader/writer states
- Mutex - Mutex with potential sleep support
- Semaphore - Counting semaphore for resource management
- Once - Single-execution primitive (like std::sync::Once)
- Barrier - Multi-thread barrier for synchronization

## Key Addresses

| Address | Description |
|---------|-------------|
| 0xFFFF_8000_0010_0000 | Kernel virtual base |
| 0xFFFF_8800_0000_0000 | Direct mapping offset |
| 0xFFFF_C900_0000_0000 | vmalloc start |
| 0x7FFF_F000_0000 | User mmap base |

## Configuration (os_cfg.toml)

The system configuration is centralized in `os_cfg.toml` and processed by the `tools/cfg` code generator.

Key configuration options:

```toml
[arch]
target = "x86_64"            # Target architecture

[qemu]
memory = "256M"              # QEMU memory size
smp = 4                      # CPU cores
machine = "q35"              # Machine type (i440fx/q35)
iommu = true                 # Enable IOMMU (requires q35)

[memory]
page_size = 4096
buddy_max_order = 11

[memory.zone]
dma_limit = 16777216        # 16 MB
dma32_limit = 4294967296    # 4 GB

[memory.pcp]
high_watermark = 64
batch_size = 16

[kernel]
phys_base = "0x100000"
direct_map_offset = "0xFFFF880000000000"
heap_init_size = 16777216   # 16 MB
stack_size = 32768          # 32 KB

[memory_model]
type = "uma"                # uma / numa

[iommu]
mode = "auto"               # off / on / auto
translation = "passthrough" # passthrough / translate
swiotlb_size = 67108864     # 64 MB

[debug]
serial = true
mm_debug = true
page_alloc_trace = true

[build]
opt_level = 3
debug_symbols = true
lto = "off"
```

## Development Status

### Completed
- [x] UEFI bootloader with GOP
- [x] Higher-half kernel (virtual memory)
- [x] GDT/IDT/TSS
- [x] APIC (Local + I/O)
- [x] Timer (PIT calibration + APIC timer)
- [x] Memory: memblock, buddy, slub, zones, PCP
- [x] Memory: VMA, vmalloc, page fault handler
- [x] ACPI parsing (MADT, DMAR, SRAT)
- [x] IOMMU (Intel VT-d with page tables)
- [x] Drivers: PS/2 keyboard, serial
- [x] Drivers: USB HID framework
- [x] Drivers: TTY (serial, console, pty)

### Next Steps
- [ ] Process/Task management
- [ ] Scheduler
- [ ] System calls (syscall instruction)
- [ ] VFS layer
- [ ] Block device drivers (AHCI/NVMe)
- [ ] Network stack
- [ ] User space

## QEMU Testing

The kernel runs with Intel VT-d emulation:
```bash
qemu-system-x86_64 \
  -machine q35,kernel-irqchip=split \
  -device intel-iommu,intremap=on \
  -drive if=pflash,format=raw,readonly=on,file=OVMF_CODE.fd \
  -drive format=raw,file=fat:rw:target/esp \
  -nographic -serial mon:stdio
```

## Shell Commands

Available in the kernel shell:
- `shutdown` - Power off
- `reboot` - Restart system
- `status` - Show uptime
- `iommu` - Show IOMMU status
- `mem` - Show memory status
- `help` - Show commands
