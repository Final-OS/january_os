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
│   │   ├── task/          # Process/Task management
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

## Development Roadmap

Based on `docs/progress/overview.md`.

### Phase 0: Foundation (Completed ✅)

**Boot & Kernel Base**
- [x] **UEFI Bootloader**: uefi-rs based with GOP graphics support.
- [x] **Kernel Entry**: Higher-half kernel mapping, direct mapping, BootInfo passing.
- [x] **Build System**: Custom config generator (`os_cfg.toml`), Makefile, QEMU integration.

**Memory Management**
- [x] **Allocators**: Memblock (early) → Buddy (page) → SLUB (object).
- [x] **Virtual Memory**: VMA management, vmalloc/vfree, Page Fault handling (COW, stack growth).
- [x] **Advanced Features**: NUMA support, PCP (Per-CPU Page Cache), IOMMU (VT-d + SWIOTLB).

**Hardware & Drivers**
- [x] **Interrupts**: GDT/TSS/IST, IDT, Local APIC, I/O APIC, APIC Timer.
- [x] **Input/Output**: PS/2 Keyboard, 16550 UART, Framebuffer Console, TTY subsystem.
- [x] **USB Stack**: xHCI controller, HID driver (Keyboard/Mouse boot protocol).
- [x] **ACPI**: Table parsing (RSDP, MADT, DMAR, SRAT, FADT).

**Synchronization**
- [x] **Primitives**: SpinLock, Mutex, RwLock, Semaphore, Once, Barrier.

### Phase 1: Process & Task Management (In Progress 🚧)

**Task Infrastructure**
- [ ] **PCB/TCB Structures**: Define Task Control Block (ID, state, context, stack).
- [ ] **Context Switching**: Implement low-level `__switch` for register saving/restoring.
- [ ] **Kernel Threads**: Support for creating and running kernel-mode threads.
- [ ] **Process Lifecycle**: Creation (`fork`/`spawn`), execution, and destruction (`exit`).

**Scheduler**
- [ ] **Framework**: Pluggable scheduler interface.
- [ ] **Algorithms**:
    - [ ] Round Robin (Basic)
    - [ ] EEVDF (Earliest Eligible Virtual Deadline First) - Linux 6.6+ style.
- [ ] **SMP Support**: Per-CPU runqueues and load balancing.

**System Calls**
- [ ] **Mechanism**: `syscall`/`sysret` instruction handling (Long Mode).
- [ ] **Infrastructure**: Syscall table dispatching and parameter handling.
- [ ] **Mode Switching**: Safe transition between User Mode (Ring 3) and Kernel Mode (Ring 0).

### Phase 2: User Space Foundation (Planned 📋)

**Memory & Loading**
- [ ] **ELF Loader**: Parse and load static ELF binaries into user address space.
- [ ] **Dynamic Linker**: Support for shared libraries (`.so`).
- [ ] **User Address Space**: VMA management for user processes (code, data, stack, heap).

**Standard Library Support**
- [ ] **C Library**: Port or support `musl` libc.
- [ ] **User Programs**: Run basic shell and utilities.

### Phase 3: File System & Storage (Planned 📋)

**Storage Stack**
- [ ] **Block Layer**: Generic block device interface and caching.
- [ ] **Drivers**:
    - [ ] virtio-blk (QEMU)
    - [ ] AHCI SATA
    - [ ] NVMe

**File Systems**
- [ ] **VFS**: Virtual File System abstraction (inodes, dentries, files).
- [ ] **Implementations**:
    - [ ] ext4 (Read/Write)
    - [ ] FAT32 (UEFI partition support)
    - [ ] procfs / sysfs (System information)

### Phase 4: Networking & Security (Planned 📋)

**Networking**
- [ ] **Drivers**: Ethernet drivers (virtio-net, e1000).
- [ ] **Stack**: TCP/IP protocol stack implementation.
- [ ] **API**: BSD Socket API compatibility.

**Security**
- [ ] **Isolation**: Strict user/kernel memory isolation (KPTI if needed).
- [ ] **Permissions**: User/Group ID checks and capability system.
- [ ] **Signals**: POSIX signal delivery and handling mechanisms.

## Configuration (os_cfg.toml)

Key configuration options:
- `[arch]`: Target architecture (x86_64).
- `[qemu]`: Memory, SMP cores, machine type.
- `[memory]`: Page size, zones, buddy allocator settings.
- `[kernel]`: Physical base, heap size, stack size.
- `[build]`: Optimization levels, debug symbols.
