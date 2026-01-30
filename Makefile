# january_os Makefile
# Builds UEFI bootloader and kernel for x86_64

.PHONY: all clean build build-boot build-kernel run qemu debug install-deps esp-tree iso help

# Architecture
ARCH := x86_64

# Directories
ROOT_DIR := $(shell pwd)
BUILD_DIR := $(ROOT_DIR)/target
ARCH_DIR := $(ROOT_DIR)/arch/$(ARCH)
KERNEL_DIR := $(ROOT_DIR)/kernel
ESP_DIR := $(BUILD_DIR)/esp

# Targets
KERNEL_TARGET := x86_64-unknown-none
BOOT_TARGET := x86_64-unknown-uefi

# Linker script (architecture-specific)
LINKER_SCRIPT := $(ARCH_DIR)/linker.ld

# Output files
BOOT_EFI := $(BUILD_DIR)/$(BOOT_TARGET)/release/january_os-boot-$(ARCH).efi
KERNEL_ELF := $(BUILD_DIR)/$(KERNEL_TARGET)/release/january_os-kernel
KERNEL_BIN := $(BUILD_DIR)/kernel.bin

# OVMF paths
OVMF_CODE := /usr/share/OVMF/OVMF_CODE_4M.fd
OVMF_CODE_ALT := /usr/share/edk2-ovmf/x64/OVMF_CODE.fd

# QEMU settings
QEMU := qemu-system-x86_64
QEMU_MEMORY := 256M
QEMU_COMMON := -m $(QEMU_MEMORY) \
	-drive if=pflash,format=raw,readonly=on,file=$(OVMF_CODE) \
	-drive format=raw,file=fat:rw:$(ESP_DIR)

QEMU_FLAGS := $(QEMU_COMMON) -serial stdio -display gtk
QEMU_FLAGS_NOGRAPHIC := $(QEMU_COMMON) -nographic -monitor none

# Default target
all: build

# Build everything
build: build-boot build-kernel create-esp
	@echo "Build complete!"
	@echo "Run 'make run' to start QEMU with GUI"
	@echo "Run 'make run-nographic' for serial console only"

# Build bootloader
build-boot:
	@echo "==> Building UEFI bootloader ($(ARCH))..."
	cargo build --release --target $(BOOT_TARGET) -p january_os-boot-$(ARCH)

# Build kernel (uses unified target directory)
build-kernel:
	@echo "==> Building kernel ($(ARCH))..."
	cd $(KERNEL_DIR) && \
		CARGO_TARGET_DIR=$(BUILD_DIR) \
		RUSTFLAGS="-C link-arg=-T$(LINKER_SCRIPT) -C link-arg=--gc-sections" \
		cargo build --release --target $(KERNEL_TARGET) \
		-Zbuild-std=core,alloc -Zbuild-std-features=compiler-builtins-mem
	@echo "==> Creating kernel binary..."
	rust-objcopy -O binary $(KERNEL_ELF) $(KERNEL_BIN)

# Create ESP (EFI System Partition) directory structure
create-esp: build-boot build-kernel
	@echo "==> Creating ESP directory structure..."
	@mkdir -p $(ESP_DIR)/EFI/BOOT
	@mkdir -p $(ESP_DIR)/EFI/january_os
	@cp $(BOOT_EFI) $(ESP_DIR)/EFI/BOOT/BOOTX64.EFI
	@cp $(KERNEL_BIN) $(ESP_DIR)/EFI/january_os/kernel.bin
	@echo "ESP created at $(ESP_DIR)"

# Show ESP tree
esp-tree: create-esp
	@echo "==> ESP Directory Structure:"
	@find $(ESP_DIR) -type f | sort

# Run in QEMU with GUI
run: build
	@echo "==> Starting QEMU (GUI mode)..."
	@if [ -f "$(OVMF_CODE)" ]; then \
		$(QEMU) $(QEMU_FLAGS); \
	elif [ -f "$(OVMF_CODE_ALT)" ]; then \
		$(QEMU) -m $(QEMU_MEMORY) -serial stdio -display gtk \
			-drive if=pflash,format=raw,readonly=on,file=$(OVMF_CODE_ALT) \
			-drive format=raw,file=fat:rw:$(ESP_DIR); \
	else \
		echo "ERROR: OVMF not found. Install with: sudo apt install ovmf"; \
		exit 1; \
	fi

# Run in QEMU without GUI (serial console only)
run-nographic: build
	@echo "==> Starting QEMU (serial console)..."
	@if [ -f "$(OVMF_CODE)" ]; then \
		$(QEMU) $(QEMU_FLAGS_NOGRAPHIC); \
	elif [ -f "$(OVMF_CODE_ALT)" ]; then \
		$(QEMU) -m $(QEMU_MEMORY) -nographic -monitor none \
			-drive if=pflash,format=raw,readonly=on,file=$(OVMF_CODE_ALT) \
			-drive format=raw,file=fat:rw:$(ESP_DIR); \
	else \
		echo "ERROR: OVMF not found. Install with: sudo apt install ovmf"; \
		exit 1; \
	fi

# Run QEMU with GDB server
debug: build
	@echo "==> Starting QEMU with GDB server on :1234..."
	@$(QEMU) $(QEMU_FLAGS) -s -S

# Shortcut
qemu: run

# Clean build artifacts
clean:
	cargo clean
	rm -rf $(ESP_DIR)
	rm -f $(KERNEL_BIN)

# Install required dependencies
install-deps:
	@echo "==> Installing Rust targets and tools..."
	rustup target add $(BOOT_TARGET)
	rustup component add rust-src llvm-tools-preview
	cargo install cargo-binutils
	@echo ""
	@echo "==> For QEMU/OVMF on Ubuntu/Debian:"
	@echo "    sudo apt install qemu-system-x86 ovmf"
	@echo ""
	@echo "==> For QEMU/OVMF on Fedora:"
	@echo "    sudo dnf install qemu-system-x86 edk2-ovmf"
	@echo ""
	@echo "==> For QEMU/OVMF on Arch:"
	@echo "    sudo pacman -S qemu-full edk2-ovmf"

# Create a simple FAT disk image for VMware (无需 sudo)
vmware-disk: build
	@echo "==> Creating VMware-compatible disk image..."
	@rm -f $(BUILD_DIR)/january_os.img
	@# 创建 32MB 空白镜像
	@dd if=/dev/zero of=$(BUILD_DIR)/january_os.img bs=1M count=32 2>/dev/null
	@# 格式化为 FAT16 (小镜像用 FAT16 更兼容)
	@mkfs.fat -F 16 $(BUILD_DIR)/january_os.img >/dev/null
	@# 使用 mtools 复制文件 (需要: sudo apt install mtools)
	@mmd -i $(BUILD_DIR)/january_os.img ::/EFI
	@mmd -i $(BUILD_DIR)/january_os.img ::/EFI/BOOT
	@mmd -i $(BUILD_DIR)/january_os.img ::/EFI/january_os
	@mcopy -i $(BUILD_DIR)/january_os.img $(BOOT_EFI) ::/EFI/BOOT/BOOTX64.EFI
	@mcopy -i $(BUILD_DIR)/january_os.img $(KERNEL_BIN) ::/EFI/january_os/kernel.bin
	@echo ""
	@echo "Disk image created: $(BUILD_DIR)/january_os.img"
	@echo ""
	@echo "VMware 使用方法:"
	@echo "  1. 创建虚拟机时选择 '使用现有虚拟磁盘'"
	@echo "  2. 或者将 .img 重命名为 .vmdk 后添加"
	@echo "  3. 确保启用 UEFI 固件"
	@ls -lh $(BUILD_DIR)/january_os.img

# Create a bootable ISO (需要 mtools: sudo apt install mtools)
iso: build
	@echo "==> Creating bootable ISO..."
	@rm -rf $(BUILD_DIR)/iso $(BUILD_DIR)/january_os.iso
	@mkdir -p $(BUILD_DIR)/iso/EFI/BOOT
	@mkdir -p $(BUILD_DIR)/iso/EFI/january_os
	@cp $(BOOT_EFI) $(BUILD_DIR)/iso/EFI/BOOT/BOOTX64.EFI
	@cp $(KERNEL_BIN) $(BUILD_DIR)/iso/EFI/january_os/kernel.bin
	@# 创建 FAT 格式的 EFI 启动镜像 (需要 mtools)
	@dd if=/dev/zero of=$(BUILD_DIR)/iso/efi.img bs=1M count=4 2>/dev/null
	@mkfs.fat -F 12 $(BUILD_DIR)/iso/efi.img >/dev/null
	@mmd -i $(BUILD_DIR)/iso/efi.img ::/EFI
	@mmd -i $(BUILD_DIR)/iso/efi.img ::/EFI/BOOT
	@mmd -i $(BUILD_DIR)/iso/efi.img ::/EFI/january_os
	@mcopy -i $(BUILD_DIR)/iso/efi.img $(BOOT_EFI) ::/EFI/BOOT/BOOTX64.EFI
	@mcopy -i $(BUILD_DIR)/iso/efi.img $(KERNEL_BIN) ::/EFI/january_os/kernel.bin
	@xorriso -as mkisofs \
		-R -J \
		-o $(BUILD_DIR)/january_os.iso \
		-e efi.img \
		-no-emul-boot \
		-isohybrid-gpt-basdat \
		$(BUILD_DIR)/iso
	@rm -f $(BUILD_DIR)/iso/efi.img
	@echo "ISO created: $(BUILD_DIR)/january_os.iso"
	@ls -lh $(BUILD_DIR)/january_os.iso

# Help
help:
	@echo "january_os Build System"
	@echo ""
	@echo "Project Structure:"
	@echo "  arch/x86_64/boot/    - UEFI bootloader"
	@echo "  arch/x86_64/linker.ld - Kernel linker script"
	@echo "  kernel/              - Kernel source"
	@echo "  target/              - Build output (unified)"
	@echo ""
	@echo "Targets:"
	@echo "  all           - Build bootloader and kernel (default)"
	@echo "  build         - Build everything and create ESP"
	@echo "  build-boot    - Build UEFI bootloader only"
	@echo "  build-kernel  - Build kernel only"
	@echo "  run           - Run in QEMU with GUI"
	@echo "  run-nographic - Run in QEMU (serial console)"
	@echo "  debug         - Run in QEMU with GDB server"
	@echo "  iso           - Create bootable ISO"
	@echo "  clean         - Clean build artifacts"
	@echo "  install-deps  - Install required tools"
	@echo "  help          - Show this help"
