# File API

文件子系统当前仍在规划阶段，本页用于承接 `open/read/write/close` 相关接口说明。

## 目标能力

- 文件描述符分配与回收
- 路径解析与打开
- 读写与 `ioctl` 统一入口

## 关键接口（规划）

```rust
pub fn vfs_open(path: &str, flags: u32, mode: u16) -> Result<i32, FsError>;
pub fn vfs_read(fd: i32, buf: &mut [u8]) -> Result<usize, FsError>;
pub fn vfs_write(fd: i32, buf: &[u8]) -> Result<usize, FsError>;
pub fn vfs_close(fd: i32) -> Result<(), FsError>;
```

## 相关文档

- [Syscall API](../syscall/syscall.md)
