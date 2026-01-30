# january_os

A simple operating system written in Rust, targeting x86_64 architecture with UEFI boot.

## Features

- UEFI bootloader using `uefi-rs`
- Minimal kernel with framebuffer and serial output
- Supports QEMU, VMware, and real hardware

## Project Structure

```
january_os/
├── arch/
│   └── x86_64/
│       ├── boot/           # UEFI bootloader
│       │   ├── Cargo.toml
│       │   └── src/main.rs
│       └── linker.ld       # Kernel linker script
├── kernel/
│   ├── Cargo.toml
│   └── src/main.rs
├── target/                 # Unified build output
├── Cargo.toml              # Workspace configuration
├── Makefile                # Build automation
└── README.md
```

## Prerequisites

### Rust Toolchain

```bash
# Install nightly Rust with required components
rustup install nightly
rustup default nightly
rustup component add rust-src llvm-tools-preview
rustup target add x86_64-unknown-uefi
cargo install cargo-binutils
```

### QEMU and OVMF

**Ubuntu/Debian:**
```bash
sudo apt install qemu-system-x86 ovmf
```

**Fedora:**
```bash
sudo dnf install qemu-system-x86 edk2-ovmf
```

**Arch Linux:**
```bash
sudo pacman -S qemu-full edk2-ovmf
```

Or use the convenience target:
```bash
make install-deps
```

## Building

```bash
# Build everything
make build

# Build only bootloader
make build-boot

# Build only kernel
make build-kernel

# Show all targets
make help
```

## Running

### QEMU with GUI
```bash
make run
```

### QEMU Serial Console (no GUI)
```bash
make run-nographic
```

### Debug with GDB
```bash
make debug
# Connect with: gdb -ex "target remote :1234"
```

### Create Bootable ISO
```bash
make iso
# Output: target/january_os.iso
```

## Architecture

### Boot Process

1. **UEFI Firmware** loads `BOOTX64.EFI` from EFI System Partition
2. **Bootloader** (`arch/x86_64/boot`):
   - Sets up graphics mode (framebuffer)
   - Loads kernel from `/EFI/january_os/kernel.bin`
   - Exits UEFI boot services
   - Jumps to kernel at 0x100000
3. **Kernel** (`kernel`):
   - Outputs to serial port (COM1)
   - Draws to framebuffer
   - Halts

### Memory Layout

| Address | Description |
|---------|-------------|
| 0x7000  | Boot info structure |
| 0x80000 | Initial kernel stack |
| 0x100000| Kernel load address |

## Roadmap

- [x] UEFI bootloader
- [x] Basic kernel with framebuffer
- [x] Serial port output
- [ ] Memory management
- [ ] Interrupt handling (IDT)
- [ ] Keyboard input
- [ ] Simple shell
- [ ] Filesystem support
- [ ] aarch64 support
- [ ] RISC-V 64 support

## License

MIT OR Apache-2.0
