# 在 VMware 中运行 january_os

## 方法一：使用 USB 启动盘 (推荐)

### 步骤

1. **准备 U 盘** (至少 64MB)

2. **在 Linux 中创建启动盘**:
   ```bash
   # 构建项目
   make build
   
   # 找到 U 盘设备 (假设是 /dev/sdb)
   lsblk
   
   # 格式化为 FAT32 并写入文件
   sudo mkfs.fat -F 32 /dev/sdb1
   sudo mount /dev/sdb1 /mnt
   sudo mkdir -p /mnt/EFI/BOOT
   sudo mkdir -p /mnt/EFI/january_os
   sudo cp target/x86_64-unknown-uefi/release/january_os-boot-x86_64.efi /mnt/EFI/BOOT/BOOTX64.EFI
   sudo cp target/kernel.bin /mnt/EFI/january_os/kernel.bin
   sudo umount /mnt
   ```

3. **在 VMware 中添加 USB 控制器**:
   - Edit Settings → Add → USB Controller
   - 连接 U 盘到虚拟机

---

## 方法二：创建虚拟硬盘

### 在 Windows 中操作

1. **创建 VHD 文件**:
   - 打开"磁盘管理" (diskmgmt.msc)
   - 操作 → 创建 VHD
   - 大小: 64 MB, 格式: VHD, 类型: 固定大小
   
2. **初始化和格式化**:
   - 右键新磁盘 → 初始化 → GPT
   - 右键未分配空间 → 新建简单卷 → FAT32
   
3. **复制文件**:
   ```
   X:\EFI\BOOT\BOOTX64.EFI    (从 target/x86_64-unknown-uefi/release/)
   X:\EFI\january_os\kernel.bin
   ```
   
4. **分离 VHD** 并在 VMware 中添加

### 在 Linux 中操作

```bash
# 安装工具
sudo apt install mtools

# 运行
make vmware-disk

# 转换为 VMDK (可选)
qemu-img convert -f raw -O vmdk target/january_os.img target/january_os.vmdk
```

---

## 方法三：直接从 ESP 目录启动 (最简单)

VMware 支持从目录启动 UEFI 应用。

### 步骤

1. **构建项目**:
   ```bash
   make build
   ```

2. **创建 VMware 虚拟机**:
   - Guest OS: Other 64-bit
   - Memory: 256 MB
   - 不添加硬盘
   - 固件: UEFI

3. **编辑 .vmx 文件**，添加:
   ```
   firmware = "efi"
   efi.legacyBoot = "FALSE"
   ```

4. **启动虚拟机**，进入 UEFI Shell

5. **手动启动** (在 UEFI Shell 中):
   ```
   Shell> fs0:
   FS0:\> cd EFI\BOOT
   FS0:\EFI\BOOT\> BOOTX64.EFI
   ```

---

## VMware 设置检查清单

- [x] 固件类型: **UEFI** (不是 BIOS)
- [x] 安全启动: **关闭** (我们的引导程序没有签名)
- [x] 内存: 至少 **128 MB**
- [x] 显示: 启用 **3D 加速** 可选

---

## 常见问题

### Q: 为什么从 CD-ROM 启动会返回 Boot Manager?

**A**: ISO 文件的 UEFI 启动结构不正确，或者 VMware 不支持该格式。
使用虚拟硬盘代替 ISO。

### Q: 显示 "No bootable device"?

**A**: 
1. 确认已启用 UEFI 固件
2. 确认 EFI 文件在正确位置: `/EFI/BOOT/BOOTX64.EFI`

### Q: 如何查看串口输出?

添加串口设备:
1. Edit Settings → Add → Serial Port
2. Use output file: `C:\Users\xxx\january_os_serial.log`
3. 用文本编辑器打开日志文件查看

---

## 使用 QEMU 代替 (更简单)

如果 VMware 有问题，QEMU 更容易使用:

```bash
# Linux
make run

# Windows (安装 QEMU 后)
qemu-system-x86_64 -bios /path/to/OVMF.fd -drive format=raw,file=fat:rw:target/esp
```
