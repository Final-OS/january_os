# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

january_os is an operating system written in Rust for x86_64 with UEFI boot, targeting Linux ABI compatibility.

**Current Version**: v0.2.0 ✅ → v0.3.0 🚧 (Filesystem/Block Devices)

---

## Project Map

```
january_os/
│
├── boot/                          # Bootloader
│   ├── x86_64/                    # ✅ UEFI boot (uefi-rs, GOP)
│   ├── aarch64/                   # ⚠️ Scaffold, not bootable
│   └── riscv64/                   # ⚠️ Scaffold, not bootable
│
├── kernel/                        # Kernel crate (no_std)
│   └── src/
│       ├── main.rs                # Kernel entry
│       ├── lib.rs                 # Kernel library root
│       ├── init.rs                # Init orchestration
│       ├── generated/             # ⚠️ Auto-generated, do not edit
│       │
│       ├── arch/x86_64/           # ✅ Architecture primitives
│       │
│       ├── mm/                    # ✅ Memory management
│       │   ├── api/               # Public interfaces
│       │   ├── runtime/           # Runtime (VMA, mm_struct)
│       │   ├── alloc/             # Allocators (Buddy, SLUB, heap)
│       │   ├── boot/              # Boot memory init
│       │   ├── virt/              # Virtual memory (paging, vmalloc)
│       │   └── arch/x86_64/       # Arch-specific page tables
│       │
│       ├── interrupt/             # ✅ Interrupt handling
│       │   ├── arch/x86_64/       # GDT, IDT, APIC, Timer
│       │   └── handlers/          # Exception/IRQ handlers
│       │
│       ├── task/                  # ✅ Process/Task management
│       │   ├── api/               # Public interfaces
│       │   ├── proc/              # Process (fork, exec, wait, signal)
│       │   ├── thread/            # Thread management
│       │   ├── sched/             # Scheduler (RR + work-stealing)
│       │   ├── syscall/           # Task-related syscalls
│       │   └── arch/x86_64/       # Context switch, syscall entry
│       │
│       ├── fs/                    # 🚧 Filesystem
│       │   ├── api/               # VFS trait definitions
│       │   ├── runtime/           # Runtime (fd table, mount mgmt)
│       │   ├── vfs/               # VFS core (path resolve, mount)
│       │   ├── fd/                # File descriptor bridge
│       │   ├── backing/           # Filesystem backends
│       │   │   ├── initramfs/     # ✅ Boot rootfs
│       │   │   ├── fat/           # ✅ FAT32 read-only
│       │   │   ├── ext4/          # ✅ ext4 read-only
│       │   │   └── mmap/          # mmap backend
│       │   └── syscall/           # File syscalls
│       │
│       ├── drivers/               # 🚧 Device drivers
│       │   ├── block/             # Block devices (virtio-blk, MBR/GPT)
│       │   ├── input/             # Keyboard/mouse
│       │   ├── serial/            # UART
│       │   ├── tty/               # TTY subsystem
│       │   ├── usb/               # xHCI + HID
│       │   └── acpi/              # ACPI table parsing
│       │
│       ├── syscall/               # ✅ Syscall framework
│       │   ├── abi/               # ABI definitions
│       │   ├── dispatch/          # Dispatch logic
│       │   └── table/             # Syscall table
│       │
│       ├── sync/                  # ✅ Synchronization primitives
│       │
│       ├── libs/                  # ✅ Kernel data structures
│       │   ├── rbtree.rs          # Red-black tree
│       │   ├── btree.rs           # B-Tree
│       │   ├── mptree.rs          # Maple Tree (VMA)
│       │   ├── rdtree.rs          # Radix Tree
│       │   └── lru.rs             # LRU cache
│       │
│       ├── smp/                   # ✅ SMP multicore
│       │
│       ├── net/                   # ⚠️ Network stack skeleton
│       ├── security/              # ⚠️ Security skeleton
│       ├── virt/                  # ⚠️ Virtualization skeleton
│       │
│       ├── shell/                 # Kernel shell
│       └── tests/                 # Kernel tests
│
├── tools/
│   ├── cfg/                       # Config generator (os_cfg.toml → Rust)
│   ├── mkinitramfs/               # initramfs packer
│   └── vmcfg/                     # VM configuration
│
├── userland/                      # User-space programs
│   ├── init/                      # PID 1 entry
│   ├── sh/                        # Basic shell
│   ├── ls/, cat/, echo/, pwd/     # Basic utilities
│   └── runtime/                   # User-space runtime
│
├── initramfs/                     # initramfs rootfs template
│
├── docs/                          # Documentation (VitePress)
│   ├── guide/                     # User/developer guides
│   ├── api/                       # API reference
│   ├── implementation/            # Implementation details
│   └── progress/                  # Development progress
│
├── skills/                        # AI Skills (Claude Code)
│
├── os_cfg.toml                    # System configuration
├── Makefile                       # Build system
└── Cargo.toml                     # Workspace root
```

### Legend

| Symbol | Meaning |
|--------|---------|
| ✅ | Complete, production-ready |
| 🚧 | In development, partially functional |
| ⚠️ | Scaffold/placeholder, not runtime-capable |

---

## Documentation Locations

### Documentation Site (`docs/`)

| Directory | Purpose | Key Files |
|-----------|---------|-----------|
| `docs/guide/` | User/developer guides | `overview.md`, `configuration.md`, `skills-info-flow.md` |
| `docs/api/` | API reference | `overview.md`, `mm/*.md`, `task/*.md`, `fs/*.md`, `sync/*.md` |
| `docs/implementation/` | Implementation details | `overview.md`, `boot.md`, `memory-init.md`, `gdt.md`, `idt.md` |
| `docs/progress/` | Development progress | `overview.md`, `roadmap.md`, `tech-debt.md`, `v0.3-plan.md` |

### Quick Reference

| Document | Path | Description |
|----------|------|-------------|
| Project Guide | `AGENTS.md` | Project structure & conventions (Chinese) |
| Claude Guide | `CLAUDE.md` | This file |
| Progress | `docs/progress/overview.md` | Current status & completed features |
| Full Roadmap | `docs/progress/roadmap.md` | v0.1 → v1.0 version planning |
| Tech Debt | `docs/progress/tech-debt.md` | Debt list & repayment plan |
| v0.3 Plan | `docs/progress/v0.3-plan.md` | Current version batch plan |
| Config Reference | `docs/guide/configuration.md` | os_cfg.toml configuration |

### Skills (`skills/`)

| Skill | Purpose |
|-------|---------|
| `january-os-project-intel/` | Project context gathering |
| `january-os-docs-skills-sync/` | Documentation sync |
| `january-os-architecture-planner/` | Architecture planning |

---

## Common Commands

```bash
make build              # Build bootloader and kernel
make build-tools        # Build config generator tool only
make build-boot         # Build only bootloader
make build-kernel       # Build only kernel
make run                # Run in QEMU (serial console, Ctrl+A X to exit)
make run-gui            # Run in QEMU with GUI
make debug              # Run with GDB server on port 1234
make iso                # Create bootable ISO (for VMware/real hardware)
make clean              # Clean build artifacts
make config             # Show current configuration from os_cfg.toml
make help               # Show all targets
```

---

## Development Roadmap

Based on `docs/progress/overview.md` and `docs/progress/roadmap.md`.

### Version Overview

```
v0.1.0 ✅ Kernel Foundation      v0.6.0   Network stack complete
v0.2.0 ✅ Process/Sched/IPC      v0.7.0   aarch64 port
v0.3.0   Filesystem/Block Dev   v0.8.0   riscv64 port
v0.4.0   Userspace complete      v0.9.0   Security hardening
v0.5.0   Network stack base      v1.0.0   Production release
```

### v0.1.0 ✅ Complete (2026-02)

- UEFI boot with GOP
- Higher-half kernel mapping
- Buddy + SLUB allocators
- Interrupt handling (IDT/APIC)
- Basic drivers (serial/keyboard/framebuffer)
- SMP multicore boot
- Kernel data structures (RbTree/LRU/RCU/RadixTree/BTree)

### v0.2.0 ✅ Complete (2026-03)

- Process/thread creation (fork/clone/vfork)
- ELF loading & ring3 switch (execve)
- wait4 event loop
- Signal mechanism (SIGCHLD/TERM/KILL/STOP/CONT)
- Pipe IPC (pipe/pipe2)
- Per-CPU scheduler + basic work-stealing
- Sync primitives (Mutex/Semaphore/CondVar)
- 40+ syscalls

### v0.3.0 🚧 In Progress (2026-Q2)

**Goal**: "Disk → Filesystem → User Program" pipeline

| Component | Status | Notes |
|-----------|--------|-------|
| BlockDevice trait | ✅ | virtio-blk driver |
| Partition parsing | ✅ | MBR/GPT basic |
| VFS core | ✅ | mount/path minimal |
| FAT32 read-only | ✅ | Basic directory/file read |
| ext4 read-only | ✅ | Basic extent read |
| execve via VFS | ✅ | ELF from filesystem |
| argv/envp/auxv | 🟡 | Minimal implementation |
| procfs | 🔴 | v0.3.1 |
| sysfs | 🔴 | v0.3.2 |

### v0.4.0 📋 Planned (2026-Q3)

- Writable FAT32/ext4
- Syscall count: 150+
- Full PT_INTERP support
- TLS (Thread Local Storage)
- initramfs / initrd

### v0.5.0 - v0.6.0 📋 Planned (2027 H1)

- TCP/IP stack
- Socket API
- virtio-net driver
- epoll / eventfd

### v0.7.0 - v0.8.0 📋 Planned (2027 H2)

- aarch64 port
- riscv64 port

### v0.9.0 - v1.0.0 📋 Planned (2028 H1)

- Security hardening (ASLR, Stack Canary)
- CFS/EEVDF scheduler
- KVM compatibility
- Production release

---

## Configuration (os_cfg.toml)

Key configuration options:

```toml
[arch]
target = "x86_64"              # Only supported architecture

[memory]
page_size = 4096               # 4KB pages
buddy_max_order = 11           # Max block: 8MB

[kernel]
phys_base = "0x0100000"        # 1MB physical base
heap_init_size = 16777216      # 16MB initial heap
stack_size = 32768             # 32KB per-CPU stack

[limits]
max_cpus = 64                  # Max CPU count

[debug]
verbose = true                 # Full boot logs
serial = true                  # Serial output
mm_debug = true                # Memory debug logs
```

---

## Coding Conventions

1. **Style**: Follow `rustfmt` defaults (4-space indentation)
2. **Naming**: `snake_case` for modules/functions, `CamelCase` for types
3. **Architecture code**: Keep in `boot/<arch>/` or `kernel/src/**/arch/<arch>/`
4. **Test code**: Place in `kernel/src/tests/**` only
5. **Generated code**: Never edit `kernel/src/generated/`

---

## Testing

No automated kernel test suite yet. Validate with:

```bash
make build           # Must compile
make run             # Must boot in QEMU
test fs fat32        # FAT32 tests
test fs ext4         # ext4 tests
test all             # All tests
```

Test scenarios must cover:
- Main path
- Key branches
- Failure paths
- Recovery paths
- Invalid/boundary input

---

## Commit Guidelines

Format: `<area>: <imperative summary>`

Examples:
- `mm: fix buddy allocator split`
- `fs: add ext4 read-only backend`
- `task: implement COW for fork`
