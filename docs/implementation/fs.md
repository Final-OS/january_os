# 文件系统实现

本文档补充 `VFS`、`FAT32`、`ext4` 在 `v0.3` 当前实现中的分层、主链路与限制口径。

## 分层结构

```text
BlockDevice
  -> partition / file-backed image
  -> VFS mount/path/inode
  -> FAT32 / ext4 / initramfs backend
  -> fs::runtime fd/cwd/dir cursor
  -> open/read/getdents/chdir/getcwd/execve
```

当前相关代码入口：

- `kernel/src/fs/vfs/`：挂载表、路径规范化、inode/filesystem 抽象
- `kernel/src/fs/runtime/manager.rs`：per-pid cwd、fd 表、mount source 解析、VFS 桥接
- `kernel/src/fs/backing/initramfs.rs`：启动 rootfs
- `kernel/src/fs/backing/fat/mod.rs`：FAT32 只读后端
- `kernel/src/fs/backing/ext4/mod.rs`：ext4 只读后端
- `kernel/src/drivers/block/file_backed.rs`：镜像文件到只读块设备的适配

## VFS 主链路

当前默认根文件系统仍是 `initramfs`，`/mnt` 只是 rootfs 内的普通目录。启动阶段会：

1. 保留 `initramfs` 作为 `/`
2. 扫描 `virtio-blk` 分区 source
3. 不自动挂载任何 FAT32/ext4
4. 允许 `ksh` 手工执行 `mount/umount/remount`
5. 允许 `execve` 直接从挂载后的 VFS 路径读取 ELF

`ksh` 当前支持两类 source：

- 已探测到的块分区，例如 `virtio-blkp1`
- `initramfs` 中的镜像文件，例如 `/mnt/fat32.img`、`/mnt/ext4.img`

相对路径会按 shell 当前 cwd 解析，因此：

```sh
cd /mnt/fat32_test
exec HELLO.ELF
```

等价于执行 `/mnt/fat32_test/HELLO.ELF`。

## FAT32 当前实现

当前能力：

- FAT32 BPB 解析
- FAT 表读取与 cluster chain 遍历
- 根目录/子目录遍历
- 普通文件读取
- 短文件名查找
- LFN 长文件名读取与查找

当前仍未实现：

- 写入、创建、删除、重命名
- FSInfo/一致性校验完善
- 更完整损坏镜像恢复语义

默认样例 FAT32 镜像可直接验证：

- `HELLO.ELF`
- `HELLO.TXT`
- `LONG-FILE.TXT`
- `README.TXT`

## ext4 当前实现

当前能力：

- superblock / group descriptor / inode table 读取
- extent tree 读取
- `depth = 0` 与 `depth > 0` 文件 extent 路径
- 目录遍历
- 基础 htree-root block 兼容

当前仍未实现或未完整实现：

- 更完整 htree 语义
- 更多 ext4 feature flag 覆盖
- 写路径
- 更系统的大文件/异常镜像稳健性

当前实现口径偏向“只读主链路可用”，不是完整 ext4 兼容层。

## 测试与样例

内核回归：

- `test fs fat32`
- `test fs ext4`

手工验证：

```sh
mount -t fat32 /mnt/fat32.img /mnt/fat32_test
ls /mnt/fat32_test
cat /mnt/fat32_test/LONG-FILE.TXT
exec /mnt/fat32_test/HELLO.ELF

mount -t ext4 /mnt/ext4.img /mnt/ext4_test
ls /mnt/ext4_test
```

## 当前技术债

- `mount` 仍只有 `ksh` 内建最小子集，缺通用 syscall 与用户态 `mount(8)`
- FAT32 仍无写路径与一致性校验完善
- ext4 仍缺完整 htree 与更多特性覆盖
- VFS 仍缺完整 dentry/mount 生命周期与缓存策略
