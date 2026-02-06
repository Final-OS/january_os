# january_os Makefile
# Configuration: os_cfg.toml
# Tools: tools/cfg

.PHONY: all build build-boot build-kernel build-tools run debug clean config help iso install-deps

# ==============================================================================
# 工具路径
# ==============================================================================
ROOT_DIR     := $(shell pwd)
BUILD_DIR    := $(ROOT_DIR)/target
KERNEL_DIR   := $(ROOT_DIR)/kernel
ESP_DIR      := $(BUILD_DIR)/esp
TOOLS_BIN    := $(ROOT_DIR)/tools/bin
CFG          := $(TOOLS_BIN)/cfg

# ==============================================================================
# 目标
# ==============================================================================
all: build

# 确保工具已构建
$(CFG): tools/cfg/src/main.rs tools/cfg/Cargo.toml
	@echo "==> Building cfg tool..."
	@mkdir -p $(TOOLS_BIN)
	@cd tools/cfg && CARGO_TARGET_DIR=/tmp/january_os_tools cargo build --release -q
	@cp /tmp/january_os_tools/release/cfg $(CFG)

# ==============================================================================
# 从配置文件读取 (需要 cfg 工具)
# ==============================================================================
# 架构配置
ARCH           = $(shell $(CFG) get arch.target)
BOOT_TARGET    = $(shell $(CFG) get arch.boot_target)
KERNEL_TARGET  = $(shell $(CFG) get arch.kernel_target)
QEMU_CMD       = $(shell $(CFG) get arch.qemu_cmd)
EFI_BOOT_FILE  = $(shell $(CFG) get arch.efi_boot_file)

# QEMU 配置
QEMU_MEMORY    = $(shell $(CFG) get qemu.memory)
QEMU_SMP       = $(shell $(CFG) get qemu.smp)
QEMU_CPU       = $(shell $(CFG) get qemu.cpu)
QEMU_KVM       = $(shell $(CFG) get qemu.kvm)
QEMU_MACHINE   = $(shell $(CFG) get qemu.machine)
QEMU_IOMMU     = $(shell $(CFG) get qemu.iommu)

# ==============================================================================
# 派生路径
# ==============================================================================
BOOT_DIR     = $(ROOT_DIR)/boot/$(ARCH)
LINKER       = $(KERNEL_DIR)/arch/$(ARCH)/linker.ld
BOOT_EFI     = $(BUILD_DIR)/$(BOOT_TARGET)/release/january_os-boot-$(ARCH).efi
KERNEL_ELF   = $(BUILD_DIR)/$(KERNEL_TARGET)/release/january_os-kernel
KERNEL_BIN   = $(BUILD_DIR)/kernel.bin

# ==============================================================================
# OVMF / KVM 检测
# ==============================================================================
OVMF := $(shell for f in /usr/share/OVMF/OVMF_CODE_4M.fd \
                         /usr/share/edk2-ovmf/x64/OVMF_CODE.fd \
                         /usr/share/OVMF/OVMF_CODE.fd; do \
            [ -f "$$f" ] && echo "$$f" && break; done)

KVM_OK := $(shell [ -w /dev/kvm ] && echo yes)
USE_KVM = $(if $(filter on auto,$(QEMU_KVM)),$(if $(KVM_OK),-enable-kvm,),)

# IOMMU 需要 Q35 机器类型和 kernel-irqchip=split
MACHINE_OPTS = $(if $(filter q35,$(QEMU_MACHINE)),-machine q35$(if $(filter true,$(QEMU_IOMMU)),$(comma)kernel-irqchip=split,),-machine $(QEMU_MACHINE))
IOMMU_OPTS = $(if $(filter true,$(QEMU_IOMMU)),-device intel-iommu,)
comma := ,

QEMU_OPTS = -m $(QEMU_MEMORY) -smp $(QEMU_SMP) $(if $(QEMU_CPU),-cpu $(QEMU_CPU),) $(USE_KVM) \
            $(MACHINE_OPTS) $(IOMMU_OPTS) \
            -device qemu-xhci,id=xhci \
            -device usb-mouse,bus=xhci.0 \
            -device usb-kbd,bus=xhci.0 \
            -drive if=pflash,format=raw,readonly=on,file=$(OVMF) \
            -drive format=raw,file=fat:rw:$(ESP_DIR)

# ==============================================================================
# Rust 编译选项 (使用 = 延迟求值，因为依赖 LINKER)
# ==============================================================================
RUSTFLAGS = -C link-arg=-T$(LINKER) -C link-arg=--gc-sections \
            -C relocation-model=static -C link-arg=-no-pie -C debuginfo=2

# ==============================================================================
# 构建目标
# ==============================================================================
build: build-boot build-kernel
	@mkdir -p $(ESP_DIR)/EFI/BOOT $(ESP_DIR)/EFI/january_os
	@cp $(BOOT_EFI) $(ESP_DIR)/EFI/BOOT/$(EFI_BOOT_FILE)
	@cp $(KERNEL_BIN) $(ESP_DIR)/EFI/january_os/kernel.bin
	@echo "\\EFI\\BOOT\\$(EFI_BOOT_FILE)" > $(ESP_DIR)/startup.nsh
	@echo "Build complete. Run 'make run' to start"

build-tools: $(CFG)

build-boot: $(CFG)
	@echo "==> Building bootloader ($(BOOT_TARGET))..."
	@cargo build --release --target $(BOOT_TARGET) -p january_os-boot-$(ARCH)

build-kernel: $(CFG)
	@echo "==> Generating config..."
	@mkdir -p $(KERNEL_DIR)/src/generated
	@$(CFG) generate $(KERNEL_DIR)/src/generated/config.rs
	@echo "mod config; pub use config::*;" > $(KERNEL_DIR)/src/generated/mod.rs
	@if [ "$(ARCH)" = "x86_64" ]; then \
		echo "==> Compiling trampoline (x86_64)..."; \
		nasm -f bin -o $(KERNEL_DIR)/src/smp/arch/x86_64/trampoline.bin $(KERNEL_DIR)/src/smp/arch/x86_64/trampoline.asm; \
	fi
	@echo "==> Building kernel ($(KERNEL_TARGET))..."
	@cd $(KERNEL_DIR) && CARGO_TARGET_DIR=$(BUILD_DIR) RUSTFLAGS="$(RUSTFLAGS)" \
		cargo build --release --target $(KERNEL_TARGET) \
		-Zbuild-std=core,alloc -Zbuild-std-features=compiler-builtins-mem
	@rust-objcopy -O binary $(KERNEL_ELF) $(KERNEL_BIN)

# ==============================================================================
# 运行目标 (纯串口模式)
# ==============================================================================
run: build
	@echo "==> $(QEMU_CMD): $(QEMU_SMP) CPUs, $(QEMU_MEMORY) RAM, KVM=$(if $(USE_KVM),on,off)"
	@echo "==> Serial console (Ctrl+A X to exit QEMU)"
	@$(QEMU_CMD) $(QEMU_OPTS) -nographic

run-gui: build
	@echo "==> $(QEMU_CMD): $(QEMU_SMP) CPUs, $(QEMU_MEMORY) RAM, KVM=$(if $(USE_KVM),on,off)"
	@echo "==> GUI console (Ctrl+A X to exit QEMU)"
	@$(QEMU_CMD) $(QEMU_OPTS) -serial stdio

debug: build
	@echo "==> GDB server on :1234 (Ctrl+A X to exit QEMU)"
	@$(QEMU_CMD) $(QEMU_OPTS) -nographic -s -S

# ==============================================================================
# 清理
# ==============================================================================
clean:
	@cargo clean
	@rm -rf $(ESP_DIR) $(KERNEL_BIN) $(KERNEL_DIR)/src/generated
	@rm -f $(KERNEL_DIR)/src/smp/arch/x86_64/trampoline.bin
	@rm -rf $(TOOLS_BIN) /tmp/january_os_tools

# ==============================================================================
# ISO 创建
# ==============================================================================
iso: build
	@echo "==> Creating bootable ISO..."
	@rm -rf $(BUILD_DIR)/iso $(BUILD_DIR)/january_os.iso
	@mkdir -p $(BUILD_DIR)/iso/EFI/BOOT $(BUILD_DIR)/iso/EFI/january_os
	@cp $(BOOT_EFI) $(BUILD_DIR)/iso/EFI/BOOT/$(EFI_BOOT_FILE)
	@cp $(KERNEL_BIN) $(BUILD_DIR)/iso/EFI/january_os/kernel.bin
	@dd if=/dev/zero of=$(BUILD_DIR)/iso/efi.img bs=1M count=8 2>/dev/null
	@mkfs.fat -F 12 $(BUILD_DIR)/iso/efi.img >/dev/null
	@mmd -i $(BUILD_DIR)/iso/efi.img ::/EFI ::/EFI/BOOT ::/EFI/january_os
	@mcopy -i $(BUILD_DIR)/iso/efi.img $(BOOT_EFI) ::/EFI/BOOT/$(EFI_BOOT_FILE)
	@mcopy -i $(BUILD_DIR)/iso/efi.img $(KERNEL_BIN) ::/EFI/january_os/kernel.bin
	@xorriso -as mkisofs -R -J -V "JANUARY_OS" -o $(BUILD_DIR)/january_os.iso \
		-e efi.img -no-emul-boot -append_partition 2 0xef $(BUILD_DIR)/iso/efi.img \
		-appended_part_as_gpt $(BUILD_DIR)/iso 2>/dev/null
	@echo "ISO: $(BUILD_DIR)/january_os.iso"
	@ls -lh $(BUILD_DIR)/january_os.iso

# ==============================================================================
# 工具安装
# ==============================================================================
install-deps: $(CFG)
	@echo "==> Installing Rust toolchain for $(ARCH)..."
	rustup target add $(shell $(CFG) get arch.rustup_target)
	rustup component add rust-src llvm-tools-preview
	cargo install cargo-binutils
	@echo ""
	@echo "==> Install QEMU, OVMF and tools:"
	@echo "  Ubuntu/Debian: sudo apt install qemu-system-x86 qemu-system-arm qemu-system-riscv ovmf mtools xorriso nasm"
	@echo "  Fedora:        sudo dnf install qemu-system-x86 qemu-system-arm qemu-system-riscv edk2-ovmf mtools xorriso nasm"
	@echo "  Arch:          sudo pacman -S qemu-full edk2-ovmf mtools xorriso nasm"

# ==============================================================================
# 配置显示
# ==============================================================================
config: $(CFG)
	@echo "=== january_os configuration ==="
	@$(CFG) show

help:
	@echo "Build:"
	@echo "  make build         - Build bootloader and kernel"
	@echo "  make build-tools   - Build helper tools only"
	@echo "  make iso           - Create bootable ISO (for VMware/real HW)"
	@echo "  make clean         - Clean build artifacts"
	@echo ""
	@echo "Run:"
	@echo "  make run           - Run in QEMU (serial console)"
	@echo "  make debug         - Run with GDB server (:1234)"
	@echo ""
	@echo "Utility:"
	@echo "  make config        - Show current configuration"
	@echo "  make install-deps  - Install required tools"
	@echo ""
	@echo "QEMU: Ctrl+A X to exit"
	@echo "Configuration: edit os_cfg.toml"
