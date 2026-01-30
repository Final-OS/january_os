# CLAUDE.md

Guidance for Claude Code when working with this repository.

## Project Overview

january_os is an operating system written in Rust for x86_64 with UEFI boot.

## Project Structure

```
january_os/
├── arch/x86_64/
│   ├── boot/           # UEFI bootloader (uefi-rs)
│   └── linker.ld       # Kernel linker script
├── kernel/             # Kernel (no_std, custom target)
├── target/             # Unified build output
├── Cargo.toml          # Workspace
└── Makefile            # Build system
```

## Common Commands

```bash
make build          # Build all
make build-boot     # Build bootloader only
make build-kernel   # Build kernel only
make run            # Run in QEMU (GUI)
make run-nographic  # Run in QEMU (serial)
make clean          # Clean build
make help           # Show all targets
```

## Build Details

- **Bootloader**: `x86_64-unknown-uefi` target, part of workspace
- **Kernel**: `x86_64-unknown-none` target, standalone with `-Zbuild-std`
- **Output**: All builds output to `target/` via `CARGO_TARGET_DIR`
- **Linker**: Architecture-specific at `arch/x86_64/linker.ld`

## Key Addresses

- `0x7000`: BootInfo structure
- `0x80000`: Kernel stack
- `0x100000`: Kernel entry point

## Boot Flow

1. UEFI loads bootloader
2. Bootloader: GOP setup → Load kernel → Exit boot services → Jump
3. Kernel: Serial init → Framebuffer draw → Halt
