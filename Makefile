# january_os Makefile
# Configuration: os_cfg.toml + vm_cfg.toml
# Tools: tools/cfg + tools/vmcfg

.PHONY: all build build-boot build-kernel build-userland build-tools run run-gui run-ksh run-gui-ksh debug debug-ksh prepare-virtio-blk prepare-initramfs clean config vm-config help iso install-deps

# ==============================================================================
# 工具路径
# ==============================================================================
ROOT_DIR     := $(shell pwd)
OS_CFG_PATH ?= $(ROOT_DIR)/os_cfg.toml
BASE_OS_CFG_PATH ?= $(ROOT_DIR)/os_cfg.toml
VM_CFG_PATH ?= $(ROOT_DIR)/vm_cfg.toml
BUILD_DIR    := $(ROOT_DIR)/target
KERNEL_DIR   := $(ROOT_DIR)/kernel
ESP_DIR      := $(BUILD_DIR)/esp
TOOLS_BIN    := $(ROOT_DIR)/tools/bin
CFG          := $(TOOLS_BIN)/cfg
VMCFG        := $(TOOLS_BIN)/vmcfg
MKINITRAMFS  := $(TOOLS_BIN)/mkinitramfs

# 从 os_cfg.toml 读取内核构建配置（通过 cfg 工具）
define oscfg_get
$(strip $(shell OS_CFG_PATH=$(OS_CFG_PATH) $(CFG) get $(1) 2>/dev/null || true))
endef

# 从 vm_cfg.toml 读取虚拟机运行配置（通过 vmcfg 工具）
define vmcfg_get
$(strip $(shell VM_CFG_PATH=$(VM_CFG_PATH) $(VMCFG) get $(1) 2>/dev/null || true))
endef

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

$(VMCFG): tools/vmcfg/vmcfg.sh
	@echo "==> Installing vmcfg tool..."
	@mkdir -p $(TOOLS_BIN)
	@cp tools/vmcfg/vmcfg.sh $(VMCFG)
	@chmod +x $(VMCFG)

$(MKINITRAMFS): tools/mkinitramfs/src/main.rs tools/mkinitramfs/Cargo.toml
	@echo "==> Building mkinitramfs tool..."
	@mkdir -p $(TOOLS_BIN)
	@cd tools/mkinitramfs && CARGO_TARGET_DIR=/tmp/january_os_tools cargo build --release -q
	@cp /tmp/january_os_tools/release/mkinitramfs $(MKINITRAMFS)

# ==============================================================================
# 从配置文件读取
# ==============================================================================
# 架构配置（cfg -> os_cfg.toml）
ARCH           = $(if $(strip $(call oscfg_get,arch.target)),$(call oscfg_get,arch.target),x86_64)
BOOT_TARGET    = $(if $(strip $(call oscfg_get,arch.boot_target)),$(call oscfg_get,arch.boot_target),x86_64-unknown-uefi)
KERNEL_TARGET  = $(if $(strip $(call oscfg_get,arch.kernel_target)),$(call oscfg_get,arch.kernel_target),x86_64-unknown-none)
EFI_BOOT_FILE  = $(if $(strip $(call oscfg_get,arch.efi_boot_file)),$(call oscfg_get,arch.efi_boot_file),BOOTX64.EFI)
QEMU_CMD       = $(if $(filter aarch64,$(ARCH)),qemu-system-aarch64,qemu-system-x86_64)

# QEMU 配置（vmcfg -> vm_cfg.toml）
QEMU_MEMORY_RAW = $(call vmcfg_get,qemu.memory)
QEMU_SMP_RAW = $(call vmcfg_get,qemu.smp)
QEMU_CPU_RAW = $(call vmcfg_get,qemu.cpu)
QEMU_KVM_RAW = $(call vmcfg_get,qemu.kvm)
QEMU_MACHINE_RAW = $(call vmcfg_get,qemu.machine)
QEMU_IOMMU_RAW = $(call vmcfg_get,qemu.iommu)
QEMU_NUMA_ARGS_RAW = $(call vmcfg_get,qemu.numa_args)
QEMU_EXTRA_ARGS_RAW = $(call vmcfg_get,qemu.extra_args)
QEMU_MEMORY    ?= $(if $(strip $(QEMU_MEMORY_RAW)),$(QEMU_MEMORY_RAW),1G)
QEMU_SMP       ?= $(if $(strip $(QEMU_SMP_RAW)),$(QEMU_SMP_RAW),4)
QEMU_CPU       ?= $(if $(strip $(QEMU_CPU_RAW)),$(QEMU_CPU_RAW),host)
QEMU_KVM       ?= $(if $(strip $(QEMU_KVM_RAW)),$(QEMU_KVM_RAW),auto)
QEMU_MACHINE   ?= $(if $(strip $(QEMU_MACHINE_RAW)),$(QEMU_MACHINE_RAW),i440fx)
QEMU_IOMMU     ?= $(if $(strip $(QEMU_IOMMU_RAW)),$(QEMU_IOMMU_RAW),false)
QEMU_NUMA_ARGS ?= $(if $(strip $(QEMU_NUMA_ARGS_RAW)),$(QEMU_NUMA_ARGS_RAW),)
QEMU_EXTRA_ARGS ?= $(if $(strip $(QEMU_EXTRA_ARGS_RAW)),$(QEMU_EXTRA_ARGS_RAW),)

# ==============================================================================
# 派生路径
# ==============================================================================
BOOT_DIR     = $(ROOT_DIR)/boot/$(ARCH)
LINKER       = $(KERNEL_DIR)/arch/$(ARCH)/linker.ld
BOOT_EFI     = $(BUILD_DIR)/$(BOOT_TARGET)/release/january_os-boot-$(ARCH).efi
KERNEL_ELF   = $(BUILD_DIR)/$(KERNEL_TARGET)/release/january_os-kernel
KERNEL_BIN   = $(BUILD_DIR)/kernel.bin
GENERATED_DIR = $(KERNEL_DIR)/src/generated
GENERATED_CONFIG = $(GENERATED_DIR)/config.rs
GENERATED_MOD = $(GENERATED_DIR)/mod.rs
TRAMPOLINE_ASM = $(KERNEL_DIR)/src/smp/arch/x86_64/trampoline.asm
TRAMPOLINE_BIN = $(KERNEL_DIR)/src/smp/arch/x86_64/trampoline.bin
INITRAMFS_CPIO = $(BUILD_DIR)/initramfs.cpio
INITRAMFS_ROOT = $(ROOT_DIR)/initramfs
INITRAMFS_STAGE = $(BUILD_DIR)/initramfs-root
USERLAND_DIR = $(ROOT_DIR)/userland
USERLAND_TARGET_DIR = $(BUILD_DIR)/userland
USERLAND_BINS = init sh ls cat pwd echo forktest
USERLAND_STAMP = $(USERLAND_TARGET_DIR)/.$(KERNEL_TARGET)-release.stamp
VIRTIO_BLK_IMG = $(BUILD_DIR)/virtio-blk.img
VIRTIO_BLK_SIZE = 64M
VIRTIO_BLK_STAGE = $(BUILD_DIR)/virtio-blk-root
VIRTIO_BLK_PART_START = 2048
VIRTIO_BLK_PART_OFFSET = 1048576
SAMPLE_FS_DIR = $(BUILD_DIR)/sample-fs
SAMPLE_FAT32_IMG = $(SAMPLE_FS_DIR)/fat32.img
SAMPLE_EXT4_IMG = $(SAMPLE_FS_DIR)/ext4.img
KSH_OS_CFG = $(BUILD_DIR)/os_cfg.ksh.toml
BOOT_SOURCES := $(shell find $(BOOT_DIR) -type f \( -name '*.rs' -o -name 'Cargo.toml' -o -name 'build.rs' \))
KERNEL_SOURCES := $(shell find $(KERNEL_DIR)/src $(KERNEL_DIR)/arch/$(ARCH) -type f ! -path '$(GENERATED_DIR)/*' ! -name 'trampoline.bin')
USERLAND_SOURCES := $(shell find $(USERLAND_DIR) -type f \( -name '*.rs' -o -name 'Cargo.toml' -o -name '*.ld' \))
INITRAMFS_SOURCES := $(shell find $(INITRAMFS_ROOT) -type f)

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
            $(QEMU_NUMA_ARGS) \
            $(QEMU_EXTRA_ARGS) \
            -device qemu-xhci,id=xhci \
            -device usb-mouse,bus=xhci.0 \
            -device usb-kbd,bus=xhci.0 \
            -drive if=pflash,format=raw,readonly=on,file=$(OVMF) \
            -drive format=raw,file=fat:rw:$(ESP_DIR) \
            -drive id=blk0,file=$(VIRTIO_BLK_IMG),format=raw,if=none \
            -device virtio-blk-pci,drive=blk0

# ==============================================================================
# Rust 编译选项 (使用 = 延迟求值，因为依赖 LINKER)
# ==============================================================================
RUSTFLAGS = -C link-arg=-T$(LINKER) -C link-arg=--gc-sections \
            -C relocation-model=static -C link-arg=-no-pie -C debuginfo=2

# ==============================================================================
# 构建目标
# ==============================================================================
build: build-boot build-kernel build-userland prepare-initramfs
	@mkdir -p $(ESP_DIR)/EFI/BOOT $(ESP_DIR)/EFI/january_os
	@cp $(BOOT_EFI) $(ESP_DIR)/EFI/BOOT/$(EFI_BOOT_FILE)
	@cp $(KERNEL_BIN) $(ESP_DIR)/EFI/january_os/kernel.bin
	@cp $(INITRAMFS_CPIO) $(ESP_DIR)/EFI/january_os/initramfs.cpio
	@echo "\\EFI\\BOOT\\$(EFI_BOOT_FILE)" > $(ESP_DIR)/startup.nsh
	@echo "Build complete. Run 'make run' to start"

build-tools: $(CFG) $(VMCFG) $(MKINITRAMFS)

build-userland: $(USERLAND_STAMP)

prepare-initramfs: $(INITRAMFS_CPIO)

$(USERLAND_STAMP): $(USERLAND_SOURCES)
	@if [ "$(ARCH)" != "x86_64" ]; then \
		echo "==> Skipping userland build for ARCH=$(ARCH)"; \
		mkdir -p $(dir $@); \
		printf 'skipped\n' > $@; \
		exit 0; \
	fi
	@echo "==> Building userland ($(KERNEL_TARGET))..."
	@CARGO_TARGET_DIR=$(USERLAND_TARGET_DIR) \
		RUSTFLAGS="-C link-arg=-T$(USERLAND_DIR)/linker.ld -C link-arg=--gc-sections -C relocation-model=static -C code-model=large -C link-arg=-no-pie -C debuginfo=1" \
		cargo build --release --manifest-path $(USERLAND_DIR)/Cargo.toml --target $(KERNEL_TARGET) \
		-Zbuild-std=core
	@mkdir -p $(dir $@)
	@touch $@

$(INITRAMFS_CPIO): Makefile $(MKINITRAMFS) $(USERLAND_STAMP) $(INITRAMFS_SOURCES)
	@mkdir -p $(BUILD_DIR)
	@rm -rf $(INITRAMFS_STAGE)
	@mkdir -p $(INITRAMFS_STAGE)
	@cp -a $(INITRAMFS_ROOT)/. $(INITRAMFS_STAGE)/
	@if [ "$(ARCH)" = "x86_64" ]; then \
		mkdir -p $(INITRAMFS_STAGE)/bin; \
		for bin in $(USERLAND_BINS); do \
			cp $(USERLAND_TARGET_DIR)/$(KERNEL_TARGET)/release/$$bin $(INITRAMFS_STAGE)/bin/$$bin; \
		done; \
	fi
	@mkdir -p $(SAMPLE_FS_DIR) $(INITRAMFS_STAGE)/mnt
	@printf 'january_os sample filesystem image\nmanual mount required\n' > $(SAMPLE_FS_DIR)/README.TXT
	@printf 'hello from sample filesystem image\n' > $(SAMPLE_FS_DIR)/HELLO.TXT
	@printf 'long filename sample\n' > $(SAMPLE_FS_DIR)/LONG-FILE.TXT
	@rm -f $(SAMPLE_FAT32_IMG)
	@truncate -s 16M $(SAMPLE_FAT32_IMG)
	@mkfs.fat -F 32 $(SAMPLE_FAT32_IMG) >/dev/null
	@mcopy -i $(SAMPLE_FAT32_IMG) $(USERLAND_TARGET_DIR)/$(KERNEL_TARGET)/release/hello ::/HELLO.ELF >/dev/null
	@mcopy -i $(SAMPLE_FAT32_IMG) $(SAMPLE_FS_DIR)/README.TXT ::/README.TXT >/dev/null
	@mcopy -i $(SAMPLE_FAT32_IMG) $(SAMPLE_FS_DIR)/HELLO.TXT ::/HELLO.TXT >/dev/null
	@mcopy -i $(SAMPLE_FAT32_IMG) $(SAMPLE_FS_DIR)/LONG-FILE.TXT ::/LONG-FILE.TXT >/dev/null
	@rm -f $(SAMPLE_EXT4_IMG)
	@truncate -s 32M $(SAMPLE_EXT4_IMG)
	@mkfs.ext4 -q -F -b 4096 $(SAMPLE_EXT4_IMG) >/dev/null
	@printf 'write %s /HELLO.ELF\nwrite %s /README.TXT\nwrite %s /HELLO.TXT\nwrite %s /LONG-FILE.TXT\n' \
		'$(USERLAND_TARGET_DIR)/$(KERNEL_TARGET)/release/hello' \
		'$(SAMPLE_FS_DIR)/README.TXT' \
		'$(SAMPLE_FS_DIR)/HELLO.TXT' \
		'$(SAMPLE_FS_DIR)/LONG-FILE.TXT' > $(SAMPLE_FS_DIR)/ext4.debugfs
	@debugfs -w -f $(SAMPLE_FS_DIR)/ext4.debugfs $(SAMPLE_EXT4_IMG) >/dev/null 2>&1
	@cp $(SAMPLE_FAT32_IMG) $(INITRAMFS_STAGE)/mnt/fat32.img
	@cp $(SAMPLE_EXT4_IMG) $(INITRAMFS_STAGE)/mnt/ext4.img
	@$(MKINITRAMFS) $(INITRAMFS_CPIO) --root $(INITRAMFS_STAGE)

build-boot: $(CFG) $(ROOT_DIR)/Cargo.toml $(ROOT_DIR)/Cargo.lock $(OS_CFG_PATH) $(BOOT_SOURCES)
	@echo "==> Building bootloader ($(BOOT_TARGET))..."
	@OS_CFG_PATH=$(OS_CFG_PATH) CARGO_TARGET_DIR=$(BUILD_DIR) cargo build --release --target $(BOOT_TARGET) -p january_os-boot-$(ARCH)

build-kernel: $(KERNEL_BIN)

$(GENERATED_CONFIG): $(CFG) $(OS_CFG_PATH)
	@echo "==> Generating config..."
	@mkdir -p $(GENERATED_DIR)
	@tmp_file=$$(mktemp); \
		OS_CFG_PATH=$(OS_CFG_PATH) $(CFG) generate $$tmp_file; \
		if [ ! -f $@ ] || ! cmp -s $$tmp_file $@; then \
			mv $$tmp_file $@; \
		else \
			rm -f $$tmp_file; \
		fi

$(GENERATED_MOD):
	@mkdir -p $(GENERATED_DIR)
	@tmp_file=$$(mktemp); \
		printf '%s\n' 'mod config; pub use config::*;' > $$tmp_file; \
		if [ ! -f $@ ] || ! cmp -s $$tmp_file $@; then \
			mv $$tmp_file $@; \
		else \
			rm -f $$tmp_file; \
		fi

$(TRAMPOLINE_BIN): $(TRAMPOLINE_ASM)
	@echo "==> Compiling trampoline (x86_64)..."
	@nasm -f bin -o $@ $<

$(KERNEL_ELF): $(CFG) $(KERNEL_DIR)/Cargo.toml $(LINKER) $(KERNEL_SOURCES) $(GENERATED_CONFIG) $(GENERATED_MOD) $(if $(filter x86_64,$(ARCH)),$(TRAMPOLINE_BIN),)
	@echo "==> Building kernel ($(KERNEL_TARGET))..."
	@cd $(KERNEL_DIR) && CARGO_TARGET_DIR=$(BUILD_DIR) RUSTFLAGS="$(RUSTFLAGS)" \
		cargo build --release --target $(KERNEL_TARGET) \
		-Zbuild-std=core,alloc -Zbuild-std-features=compiler-builtins-mem

$(KERNEL_BIN): $(KERNEL_ELF)
	@rust-objcopy -O binary $< $@

# ============================================================================== 
# 运行目标 (纯串口模式)
# ============================================================================== 
prepare-virtio-blk: Makefile $(USERLAND_STAMP)
	@mkdir -p $(BUILD_DIR)
	@rm -f $(VIRTIO_BLK_IMG)
	@truncate -s $(VIRTIO_BLK_SIZE) $(VIRTIO_BLK_IMG)
	@if [ "$(ARCH)" != "x86_64" ]; then \
		echo "==> Created blank virtio-blk image for ARCH=$(ARCH): $(VIRTIO_BLK_IMG) ($(VIRTIO_BLK_SIZE))"; \
		exit 0; \
	fi
	@rm -rf $(VIRTIO_BLK_STAGE)
	@mkdir -p $(VIRTIO_BLK_STAGE)
	@printf 'january_os default data disk\nmanual mount required\nsample ELF: HELLO.ELF = userland/hello\n' > $(VIRTIO_BLK_STAGE)/README.TXT
	@printf 'hello from default FAT32 data disk\n' > $(VIRTIO_BLK_STAGE)/HELLO.TXT
	@printf 'long filename sample\n' > $(VIRTIO_BLK_STAGE)/LONG-FILE.TXT
	@cp $(USERLAND_TARGET_DIR)/$(KERNEL_TARGET)/release/hello $(VIRTIO_BLK_STAGE)/HELLO.ELF
	@printf 'label: dos\nunit: sectors\n\n$(VIRTIO_BLK_PART_START),,c,*\n' | sfdisk $(VIRTIO_BLK_IMG) >/dev/null
	@mkfs.fat -F 32 --offset=$(VIRTIO_BLK_PART_START) $(VIRTIO_BLK_IMG) >/dev/null
	@for file in $(VIRTIO_BLK_STAGE)/*; do \
		name=$$(basename $$file); \
		mcopy -i $(VIRTIO_BLK_IMG)@@$(VIRTIO_BLK_PART_OFFSET) $$file ::/$$name >/dev/null; \
	done
	@echo "==> Created virtio-blk image: $(VIRTIO_BLK_IMG) ($(VIRTIO_BLK_SIZE), MBR + FAT32 sample data image)"

$(KSH_OS_CFG): $(BASE_OS_CFG_PATH)
	@mkdir -p $(BUILD_DIR)
	@sed 's#^initrd_command = .*#initrd_command = "ksh"#' $(BASE_OS_CFG_PATH) > $@

run: $(VMCFG) build prepare-virtio-blk
	@echo "==> $(QEMU_CMD): $(QEMU_SMP) CPUs, $(QEMU_MEMORY) RAM, CPU='$(QEMU_CPU)', KVM=$(if $(USE_KVM),on,off), NUMA=$(if $(strip $(QEMU_NUMA_ARGS)),on,off)"
	@echo "==> Serial console (Ctrl+A X to exit QEMU)"
	@$(QEMU_CMD) $(QEMU_OPTS) -nographic

run-ksh: $(KSH_OS_CFG)
	@$(MAKE) BASE_OS_CFG_PATH=$(BASE_OS_CFG_PATH) OS_CFG_PATH=$(KSH_OS_CFG) run

run-gui: $(VMCFG) build prepare-virtio-blk
	@echo "==> $(QEMU_CMD): $(QEMU_SMP) CPUs, $(QEMU_MEMORY) RAM, CPU='$(QEMU_CPU)', KVM=$(if $(USE_KVM),on,off), NUMA=$(if $(strip $(QEMU_NUMA_ARGS)),on,off)"
	@echo "==> GUI console (Ctrl+A X to exit QEMU)"
	@$(QEMU_CMD) $(QEMU_OPTS) -serial stdio

run-gui-ksh: $(KSH_OS_CFG)
	@$(MAKE) BASE_OS_CFG_PATH=$(BASE_OS_CFG_PATH) OS_CFG_PATH=$(KSH_OS_CFG) run-gui

debug: $(VMCFG) build prepare-virtio-blk
	@echo "==> GDB server on :1234 (Ctrl+A X to exit QEMU)"
	@$(QEMU_CMD) $(QEMU_OPTS) -nographic -s -S

debug-ksh: $(KSH_OS_CFG)
	@$(MAKE) BASE_OS_CFG_PATH=$(BASE_OS_CFG_PATH) OS_CFG_PATH=$(KSH_OS_CFG) debug

# ==============================================================================
# 清理
# ==============================================================================
clean:
	@cargo clean
	@rm -rf $(ESP_DIR) $(KERNEL_BIN) $(KERNEL_DIR)/src/generated
	@rm -f $(INITRAMFS_CPIO)
	@rm -rf $(INITRAMFS_STAGE)
	@rm -rf $(VIRTIO_BLK_STAGE)
	@rm -f $(VIRTIO_BLK_IMG)
	@rm -f $(KSH_OS_CFG)
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
	@cp $(INITRAMFS_CPIO) $(BUILD_DIR)/iso/EFI/january_os/initramfs.cpio
	@dd if=/dev/zero of=$(BUILD_DIR)/iso/efi.img bs=1M count=8 2>/dev/null
	@mkfs.fat -F 12 $(BUILD_DIR)/iso/efi.img >/dev/null
	@mmd -i $(BUILD_DIR)/iso/efi.img ::/EFI ::/EFI/BOOT ::/EFI/january_os
	@mcopy -i $(BUILD_DIR)/iso/efi.img $(BOOT_EFI) ::/EFI/BOOT/$(EFI_BOOT_FILE)
	@mcopy -i $(BUILD_DIR)/iso/efi.img $(KERNEL_BIN) ::/EFI/january_os/kernel.bin
	@mcopy -i $(BUILD_DIR)/iso/efi.img $(INITRAMFS_CPIO) ::/EFI/january_os/initramfs.cpio
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
	rustup target add $(if $(strip $(call oscfg_get,arch.rustup_target)),$(call oscfg_get,arch.rustup_target),x86_64-unknown-uefi)
	rustup component add rust-src llvm-tools-preview
	cargo install cargo-binutils
	@echo ""
	@echo "==> Detecting Linux distribution and installing system packages..."
	@if [ ! -f /etc/os-release ]; then \
		echo "  ERROR: /etc/os-release not found, cannot detect distribution."; \
		echo "  Please install the following packages manually:"; \
		echo "    QEMU (x86/arm/riscv), OVMF/EDK2, mtools, xorriso, nasm"; \
		exit 1; \
	fi; \
	. /etc/os-release; \
	DISTRO_INFO="$$ID_LIKE $$ID"; \
	echo "  Distribution: $$PRETTY_NAME (ID=$$ID, ID_LIKE=$$ID_LIKE)"; \
	case "$$DISTRO_INFO" in \
		*ubuntu* | *debian*) \
			echo "  Package manager: apt"; \
			sudo apt-get update -q; \
			sudo apt-get install -y --no-install-recommends \
				qemu-system-x86 qemu-system-arm qemu-system-riscv \
				ovmf mtools xorriso nasm ;; \
		*fedora* | *rhel* | *centos*) \
			echo "  Package manager: dnf"; \
			sudo dnf install -y \
				qemu-system-x86 qemu-system-arm qemu-system-riscv \
				edk2-ovmf mtools xorriso nasm ;; \
		*arch*) \
			echo "  Package manager: pacman"; \
			sudo pacman -S --needed --noconfirm \
				qemu-full edk2-ovmf mtools xorriso nasm ;; \
		*suse*) \
			echo "  Package manager: zypper"; \
			sudo zypper install -y \
				qemu-x86 qemu-arm qemu-extra ovmf mtools xorriso nasm ;; \
		*) \
			echo "  WARNING: Unsupported distribution '$$PRETTY_NAME', skipping automatic install."; \
			echo "  Please install the following packages manually:"; \
			echo "    Ubuntu/Debian: sudo apt install qemu-system-x86 qemu-system-arm qemu-system-riscv ovmf mtools xorriso nasm"; \
			echo "    Fedora:        sudo dnf install qemu-system-x86 qemu-system-arm qemu-system-riscv edk2-ovmf mtools xorriso nasm"; \
			echo "    Arch:          sudo pacman -S qemu-full edk2-ovmf mtools xorriso nasm"; \
			echo "    openSUSE:      sudo zypper install qemu-x86 qemu-arm qemu-extra ovmf mtools xorriso nasm" ;; \
	esac
	@echo ""
	@echo "==> install-deps complete."

# ==============================================================================
# 配置显示
# ==============================================================================
config: $(CFG)
	@echo "=== january_os kernel configuration (os_cfg.toml) ==="
	@OS_CFG_PATH=$(OS_CFG_PATH) $(CFG) show

vm-config: $(VMCFG)
	@echo "=== january_os VM configuration (vm_cfg.toml) ==="
	@VM_CFG_PATH=$(VM_CFG_PATH) $(VMCFG) show

help:
	@echo "Build:"
	@echo "  make build         - Build bootloader and kernel"
	@echo "  make build-tools   - Build helper tools only"
	@echo "  make iso           - Create bootable ISO (for VMware/real HW)"
	@echo "  make clean         - Clean build artifacts"
	@echo ""
	@echo "Run:"
	@echo "  make run           - Run in QEMU (serial console)"
	@echo "  make run-gui       - Run in QEMU (GUI console)"
	@echo "  make run-ksh       - Run in QEMU and boot directly into kernel shell"
	@echo "  make run-gui-ksh   - GUI run and boot directly into kernel shell"
	@echo "  edit vm_cfg.toml   - Set qemu.cpu / qemu.numa_args / qemu.extra_args"
	@echo ""
	@echo "Debug:"
	@echo "  make debug         - Run with GDB server (:1234)"
	@echo "  make debug-ksh     - Debug boot directly into kernel shell"
	@echo ""
	@echo "Utility:"
	@echo "  make config        - Show current kernel configuration (os_cfg.toml)"
	@echo "  make vm-config     - Show current VM configuration (vm_cfg.toml)"
	@echo "  make install-deps  - Install required tools"
	@echo ""
	@echo "QEMU: Ctrl+A X to exit"
	@echo "Configuration: edit os_cfg.toml and vm_cfg.toml"
