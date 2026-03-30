use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use crate::drivers::block::BlockDevice;
use crate::fs::api::{DirEntry, FileType, FsError, Metadata};
use crate::fs::runtime::manager::FsStatFs;
use crate::fs::vfs::{FileSystem, Inode};

const EXT4_SUPERBLOCK_OFFSET: usize = 1024;
const EXT4_SUPER_MAGIC: u16 = 0xEF53;
const EXT4_EXTENTS_FL: u32 = 0x0008_0000;
const EXT4_EXTENT_MAGIC: u16 = 0xF30A;
const EXT4_FT_REG_FILE: u8 = 1;
const EXT4_FT_DIR: u8 = 2;
const EXT4_S_IFDIR: u16 = 0x4000;
const EXT4_S_IFREG: u16 = 0x8000;

#[derive(Clone)]
pub struct Ext4FileSystem {
    device: Arc<dyn BlockDevice>,
    block_size: u32,
    blocks_count: u64,
    free_blocks_count: u64,
    inodes_count: u64,
    free_inodes_count: u64,
    inode_size: u16,
    inodes_per_group: u32,
    group_desc_size: u16,
    group_desc_block: u64,
}

#[derive(Clone)]
struct Ext4Node {
    ino: u32,
    mode: u16,
    size: u64,
    flags: u32,
    block: [u8; 60],
}

#[derive(Clone)]
struct Ext4Inode {
    fs: Arc<Ext4FileSystem>,
    node: Ext4Node,
}

#[derive(Clone, Copy)]
struct Extent {
    logical: u32,
    len: u16,
    phys: u64,
}

#[derive(Clone, Copy)]
struct ExtentIndex {
    logical: u32,
    phys: u64,
}

impl Ext4FileSystem {
    pub fn mount(device: Arc<dyn BlockDevice>) -> Result<Arc<dyn FileSystem>, FsError> {
        let sector_size = device.block_size() as usize;
        if sector_size == 0 || 4096 % sector_size != 0 {
            return Err(FsError::InvalidInput);
        }

        let superblock = read_bytes(device.clone(), EXT4_SUPERBLOCK_OFFSET, 1024)?;
        let magic = le_u16(&superblock[56..58]);
        if magic != EXT4_SUPER_MAGIC {
            return Err(FsError::InvalidInput);
        }

        let log_block_size = le_u32(&superblock[24..28]);
        let block_size = 1024u32
            .checked_shl(log_block_size)
            .ok_or(FsError::InvalidInput)?;
        if block_size != 4096 {
            return Err(FsError::NotSupported);
        }

        let inode_size = le_u16(&superblock[88..90]);
        let inodes_per_group = le_u32(&superblock[40..44]);
        let blocks_lo = le_u32(&superblock[4..8]) as u64;
        let free_blocks_lo = le_u32(&superblock[12..16]) as u64;
        let free_inodes_lo = le_u32(&superblock[16..20]) as u64;
        let blocks_hi = le_u32(&superblock[336..340]) as u64;
        let free_blocks_hi = le_u32(&superblock[344..348]) as u64;
        let blocks_count = blocks_lo | (blocks_hi << 32);
        let free_blocks_count = free_blocks_lo | (free_blocks_hi << 32);
        let inodes_count = le_u32(&superblock[0..4]) as u64;
        let desc_size = {
            let raw = le_u16(&superblock[254..256]);
            if raw == 0 { 32 } else { raw.max(32) }
        };

        if inode_size < 128 || inodes_per_group == 0 || desc_size < 32 {
            return Err(FsError::InvalidInput);
        }

        Ok(Arc::new(Self {
            device,
            block_size,
            blocks_count,
            free_blocks_count,
            inodes_count,
            free_inodes_count: free_inodes_lo,
            inode_size,
            inodes_per_group,
            group_desc_size: desc_size,
            group_desc_block: 1,
        }))
    }

    fn read_block(&self, block: u64) -> Result<Vec<u8>, FsError> {
        read_bytes(
            self.device.clone(),
            block as usize * self.block_size as usize,
            self.block_size as usize,
        )
    }

    fn read_group_desc(&self, group: u32) -> Result<Vec<u8>, FsError> {
        let offset = self.group_desc_block as usize * self.block_size as usize
            + group as usize * self.group_desc_size as usize;
        read_bytes(self.device.clone(), offset, self.group_desc_size as usize)
    }

    fn read_inode_raw(&self, ino: u32) -> Result<Vec<u8>, FsError> {
        if ino == 0 {
            return Err(FsError::InvalidInput);
        }
        let group = (ino - 1) / self.inodes_per_group;
        let index = (ino - 1) % self.inodes_per_group;
        let desc = self.read_group_desc(group)?;
        let inode_table = le_u32(&desc[8..12]) as usize;
        let inode_offset = inode_table
            .checked_mul(self.block_size as usize)
            .and_then(|base| base.checked_add(index as usize * self.inode_size as usize))
            .ok_or(FsError::InvalidInput)?;
        read_bytes(self.device.clone(), inode_offset, self.inode_size as usize)
    }

    fn read_inode(&self, ino: u32) -> Result<Ext4Node, FsError> {
        let raw = self.read_inode_raw(ino)?;
        let mode = le_u16(&raw[0..2]);
        let size_lo = le_u32(&raw[4..8]) as u64;
        let flags = le_u32(&raw[32..36]);
        let size_high = le_u32(&raw[108..112]) as u64;
        let size = size_lo | (size_high << 32);
        let mut block = [0u8; 60];
        block.copy_from_slice(&raw[40..100]);

        Ok(Ext4Node {
            ino,
            mode,
            size,
            flags,
            block,
        })
    }

    fn inode_file_type(node: &Ext4Node) -> Result<FileType, FsError> {
        match node.mode & 0xF000 {
            EXT4_S_IFDIR => Ok(FileType::Directory),
            EXT4_S_IFREG => Ok(FileType::Regular),
            _ => Err(FsError::NotSupported),
        }
    }

    fn extent_map(&self, node: &Ext4Node) -> Result<Vec<Extent>, FsError> {
        if (node.flags & EXT4_EXTENTS_FL) == 0 {
            return Err(FsError::NotSupported);
        }
        self.extent_map_from_bytes(&node.block)
    }

    fn extent_map_from_bytes(&self, raw: &[u8]) -> Result<Vec<Extent>, FsError> {
        if raw.len() < 12 {
            return Err(FsError::InvalidInput);
        }
        let magic = le_u16(&raw[0..2]);
        let entries = le_u16(&raw[2..4]) as usize;
        let depth = le_u16(&raw[6..8]);
        if magic != EXT4_EXTENT_MAGIC {
            return Err(FsError::InvalidInput);
        }

        if depth == 0 {
            let mut out = Vec::new();
            let mut offset = 12usize;
            for _ in 0..entries {
                if offset + 12 > raw.len() {
                    return Err(FsError::InvalidInput);
                }
                let logical = le_u32(&raw[offset..offset + 4]);
                let len = le_u16(&raw[offset + 4..offset + 6]);
                let start_hi = le_u16(&raw[offset + 6..offset + 8]) as u64;
                let start_lo = le_u32(&raw[offset + 8..offset + 12]) as u64;
                if (len & 0x8000) != 0 {
                    return Err(FsError::NotSupported);
                }
                out.push(Extent {
                    logical,
                    len,
                    phys: (start_hi << 32) | start_lo,
                });
                offset += 12;
            }
            return Ok(out);
        }

        let mut indices = Vec::new();
        let mut offset = 12usize;
        for _ in 0..entries {
            if offset + 12 > raw.len() {
                return Err(FsError::InvalidInput);
            }
            let logical = le_u32(&raw[offset..offset + 4]);
            let leaf_lo = le_u32(&raw[offset + 4..offset + 8]) as u64;
            let leaf_hi = le_u16(&raw[offset + 8..offset + 10]) as u64;
            indices.push(ExtentIndex {
                logical,
                phys: (leaf_hi << 32) | leaf_lo,
            });
            offset += 12;
        }

        let mut out = Vec::new();
        for index in indices {
            let child = self.read_block(index.phys)?;
            let child_extents = self.extent_map_from_bytes(child.as_slice())?;
            if child_extents
                .first()
                .map(|extent| extent.logical)
                .unwrap_or(index.logical)
                < index.logical
            {
                return Err(FsError::InvalidInput);
            }
            out.extend(child_extents);
        }
        out.sort_by_key(|extent| extent.logical);
        Ok(out)
    }

    fn read_node_at(
        &self,
        node: &Ext4Node,
        offset: usize,
        out: &mut [u8],
    ) -> Result<usize, FsError> {
        if matches!(Self::inode_file_type(node)?, FileType::Directory) {
            return Err(FsError::IsDirectory);
        }
        if out.is_empty() || offset >= node.size as usize {
            return Ok(0);
        }

        let extents = self.extent_map(node)?;
        let block_size = self.block_size as usize;
        let wanted = out.len().min(node.size as usize - offset);
        let mut written = 0usize;
        let start_block = offset / block_size;
        let mut block_off = offset % block_size;

        for extent in extents {
            let extent_start = extent.logical as usize;
            let extent_end = extent_start + extent.len as usize;
            if start_block >= extent_end {
                continue;
            }
            let mut logical = start_block.max(extent_start);
            while logical < extent_end && written < wanted {
                let phys = extent.phys + (logical - extent_start) as u64;
                let data = self.read_block(phys)?;
                let start = if logical == start_block { block_off } else { 0 };
                let count = (block_size - start).min(wanted - written);
                out[written..written + count].copy_from_slice(&data[start..start + count]);
                written += count;
                logical += 1;
                block_off = 0;
            }
        }

        Ok(written)
    }

    fn read_dir_entries(&self, node: &Ext4Node) -> Result<Vec<DirEntry>, FsError> {
        if !matches!(Self::inode_file_type(node)?, FileType::Directory) {
            return Err(FsError::NotDirectory);
        }

        let mut buf = vec![0u8; node.size as usize];
        let n = self.read_dir_data(node, &mut buf)?;
        buf.truncate(n);

        let mut out = Vec::new();
        let block_size = self.block_size as usize;
        for block in buf.chunks(block_size) {
            let mut offset = 0usize;
            while offset + 8 <= block.len() {
                let ino = le_u32(&block[offset..offset + 4]);
                let rec_len = le_u16(&block[offset + 4..offset + 6]) as usize;
                let name_len = block[offset + 6] as usize;
                let file_type = block[offset + 7];
                if rec_len == 0 || offset + rec_len > block.len() || name_len + 8 > rec_len {
                    break;
                }
                if ino != 0 {
                    let name = core::str::from_utf8(&block[offset + 8..offset + 8 + name_len])
                        .map_err(|_| FsError::InvalidInput)?
                        .to_string();
                    if name != "." && name != ".." {
                        out.push(DirEntry {
                            ino: ino as u64,
                            file_type: match file_type {
                                EXT4_FT_DIR => FileType::Directory,
                                EXT4_FT_REG_FILE => FileType::Regular,
                                _ => FileType::Unknown,
                            },
                            name,
                        });
                    }
                }
                offset += rec_len;
            }
        }
        Ok(out)
    }

    fn read_dir_data(&self, node: &Ext4Node, out: &mut [u8]) -> Result<usize, FsError> {
        let extents = self.extent_map(node)?;
        let mut written = 0usize;
        let mut remaining = out.len();
        for extent in extents {
            for idx in 0..extent.len as usize {
                if remaining == 0 {
                    return Ok(written);
                }
                let data = self.read_block(extent.phys + idx as u64)?;
                let count = remaining.min(data.len());
                out[written..written + count].copy_from_slice(&data[..count]);
                written += count;
                remaining -= count;
            }
        }
        Ok(written)
    }
}

impl FileSystem for Ext4FileSystem {
    fn name(&self) -> &str {
        "ext4"
    }

    fn root(&self) -> Arc<dyn Inode> {
        let root = self.read_inode(2).expect("ext4 root inode");
        Arc::new(Ext4Inode {
            fs: Arc::new(self.clone()),
            node: root,
        })
    }

    fn sync(&self) -> Result<(), FsError> {
        Ok(())
    }

    fn statfs(&self) -> Result<FsStatFs, FsError> {
        Ok(FsStatFs {
            f_type: EXT4_SUPER_MAGIC as i64,
            f_bsize: self.block_size as i64,
            f_blocks: self.blocks_count,
            f_bfree: self.free_blocks_count,
            f_bavail: self.free_blocks_count,
            f_files: self.inodes_count,
            f_ffree: self.free_inodes_count,
            f_namelen: 255,
            f_frsize: self.block_size as i64,
            f_flags: if self.device.is_read_only() { 1 } else { 0 },
        })
    }
}

impl Inode for Ext4Inode {
    fn metadata(&self) -> Result<Metadata, FsError> {
        Ok(Metadata {
            ino: self.node.ino as u64,
            file_type: Ext4FileSystem::inode_file_type(&self.node)?,
            mode: self.node.mode as u32,
            size: self.node.size,
            nlink: if matches!(
                Ext4FileSystem::inode_file_type(&self.node)?,
                FileType::Directory
            ) {
                2
            } else {
                1
            },
        })
    }

    fn lookup(&self, name: &str) -> Result<Arc<dyn Inode>, FsError> {
        if name == "." {
            return Ok(Arc::new(self.clone()));
        }
        for entry in self.fs.read_dir_entries(&self.node)? {
            if entry.name == name {
                let node = self.fs.read_inode(entry.ino as u32)?;
                return Ok(Arc::new(Self {
                    fs: self.fs.clone(),
                    node,
                }));
            }
        }
        Err(FsError::NotFound)
    }

    fn readdir(&self) -> Result<Vec<DirEntry>, FsError> {
        self.fs.read_dir_entries(&self.node)
    }

    fn read_at(&self, offset: usize, out: &mut [u8]) -> Result<usize, FsError> {
        self.fs.read_node_at(&self.node, offset, out)
    }
}

fn read_bytes(device: Arc<dyn BlockDevice>, offset: usize, len: usize) -> Result<Vec<u8>, FsError> {
    let block_size = device.block_size() as usize;
    let start_lba = offset / block_size;
    let end = offset.checked_add(len).ok_or(FsError::InvalidInput)?;
    let end_lba = end.div_ceil(block_size);
    let mut block = vec![0u8; block_size];
    let mut out = vec![0u8; len];
    let mut written = 0usize;

    for lba in start_lba..end_lba {
        device
            .read_block(lba as u64, &mut block)
            .map_err(|_| FsError::Io)?;
        let block_start = lba * block_size;
        let copy_start = offset.saturating_sub(block_start);
        let copy_end = (end - block_start).min(block_size);
        let count = copy_end.saturating_sub(copy_start);
        out[written..written + count].copy_from_slice(&block[copy_start..copy_start + count]);
        written += count;
    }

    Ok(out)
}

fn le_u16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[0], bytes[1]])
}

fn le_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}
