use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::char;

use crate::drivers::block::BlockDevice;
use crate::fs::api::{DirEntry, FileType, FsError, Metadata};
use crate::fs::vfs::{FileSystem, Inode};

const FAT32_SIGNATURE0: u8 = 0x55;
const FAT32_SIGNATURE1: u8 = 0xAA;
const FAT32_ATTR_DIRECTORY: u8 = 0x10;
const FAT32_ATTR_VOLUME_ID: u8 = 0x08;
const FAT32_ATTR_LFN: u8 = 0x0F;
const FAT32_EOC: u32 = 0x0FFF_FFF8;

#[derive(Clone, Copy)]
struct BiosParameterBlock {
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sectors: u16,
    num_fats: u8,
    total_sectors: u32,
    fat_size_sectors: u32,
    root_cluster: u32,
}

impl BiosParameterBlock {
    fn cluster_size(&self) -> usize {
        self.bytes_per_sector as usize * self.sectors_per_cluster as usize
    }
}

#[derive(Clone)]
pub struct Fat32FileSystem {
    device: Arc<dyn BlockDevice>,
    bpb: BiosParameterBlock,
    fat: Arc<Vec<u32>>,
    first_data_sector: u64,
    cluster_count: u32,
}

#[derive(Clone)]
struct Fat32Node {
    ino: u64,
    cluster: u32,
    size: u64,
    is_dir: bool,
}

#[derive(Clone)]
struct Fat32Inode {
    fs: Arc<Fat32FileSystem>,
    node: Fat32Node,
}

#[derive(Clone)]
struct FatDirRecord {
    ino: u64,
    name: String,
    cluster: u32,
    size: u64,
    is_dir: bool,
}

impl Fat32FileSystem {
    pub fn mount(device: Arc<dyn BlockDevice>) -> Result<Arc<dyn FileSystem>, FsError> {
        let bpb = Self::parse_bpb(device.clone())?;
        let fat = Self::read_fat(device.clone(), &bpb)?;
        let first_data_sector =
            bpb.reserved_sectors as u64 + (bpb.num_fats as u64 * bpb.fat_size_sectors as u64);
        let data_sectors = bpb
            .total_sectors
            .checked_sub(first_data_sector as u32)
            .ok_or(FsError::InvalidInput)?;
        let cluster_count = data_sectors / bpb.sectors_per_cluster as u32;
        if bpb.root_cluster < 2 || cluster_count < 2 {
            return Err(FsError::InvalidInput);
        }

        Ok(Arc::new(Self {
            device,
            bpb,
            fat: Arc::new(fat),
            first_data_sector,
            cluster_count,
        }))
    }

    fn parse_bpb(device: Arc<dyn BlockDevice>) -> Result<BiosParameterBlock, FsError> {
        let block_size = device.block_size() as usize;
        if block_size < 512 {
            return Err(FsError::InvalidInput);
        }
        let mut boot = vec![0u8; block_size];
        device.read_block(0, &mut boot).map_err(|_| FsError::Io)?;

        if boot[510] != FAT32_SIGNATURE0 || boot[511] != FAT32_SIGNATURE1 {
            return Err(FsError::InvalidInput);
        }

        let bytes_per_sector = le_u16(&boot[11..13]);
        let sectors_per_cluster = boot[13];
        let reserved_sectors = le_u16(&boot[14..16]);
        let num_fats = boot[16];
        let root_entry_count = le_u16(&boot[17..19]);
        let total16 = le_u16(&boot[19..21]) as u32;
        let fat_size16 = le_u16(&boot[22..24]) as u32;
        let total32 = le_u32(&boot[32..36]);
        let fat_size32 = le_u32(&boot[36..40]);
        let root_cluster = le_u32(&boot[44..48]);

        if bytes_per_sector as usize != block_size
            || sectors_per_cluster == 0
            || reserved_sectors == 0
            || num_fats == 0
            || root_entry_count != 0
            || fat_size16 != 0
            || fat_size32 == 0
        {
            return Err(FsError::InvalidInput);
        }

        let total_sectors = if total16 != 0 { total16 } else { total32 };
        if total_sectors == 0 {
            return Err(FsError::InvalidInput);
        }

        Ok(BiosParameterBlock {
            bytes_per_sector,
            sectors_per_cluster,
            reserved_sectors,
            num_fats,
            total_sectors,
            fat_size_sectors: fat_size32,
            root_cluster,
        })
    }

    fn read_fat(device: Arc<dyn BlockDevice>, bpb: &BiosParameterBlock) -> Result<Vec<u32>, FsError> {
        let block_size = bpb.bytes_per_sector as usize;
        let mut fat_bytes = vec![0u8; bpb.fat_size_sectors as usize * block_size];
        let mut sector = vec![0u8; block_size];
        for idx in 0..bpb.fat_size_sectors {
            device
                .read_block(bpb.reserved_sectors as u64 + idx as u64, &mut sector)
                .map_err(|_| FsError::Io)?;
            let off = idx as usize * block_size;
            fat_bytes[off..off + block_size].copy_from_slice(&sector);
        }

        let mut entries = Vec::with_capacity(fat_bytes.len() / 4);
        let mut offset = 0usize;
        while offset + 4 <= fat_bytes.len() {
            entries.push(le_u32(&fat_bytes[offset..offset + 4]) & 0x0FFF_FFFF);
            offset += 4;
        }
        Ok(entries)
    }

    fn cluster_to_lba(&self, cluster: u32) -> Result<u64, FsError> {
        if cluster < 2 || cluster >= self.cluster_count + 2 {
            return Err(FsError::InvalidInput);
        }
        Ok(self.first_data_sector
            + (cluster as u64 - 2) * self.bpb.sectors_per_cluster as u64)
    }

    fn read_cluster(&self, cluster: u32) -> Result<Vec<u8>, FsError> {
        let mut out = vec![0u8; self.bpb.cluster_size()];
        let lba = self.cluster_to_lba(cluster)?;
        let sector_size = self.bpb.bytes_per_sector as usize;
        let mut sector = vec![0u8; sector_size];
        for idx in 0..self.bpb.sectors_per_cluster as usize {
            self.device
                .read_block(lba + idx as u64, &mut sector)
                .map_err(|_| FsError::Io)?;
            let off = idx * sector_size;
            out[off..off + sector_size].copy_from_slice(&sector);
        }
        Ok(out)
    }

    fn next_cluster(&self, cluster: u32) -> Result<Option<u32>, FsError> {
        let Some(value) = self.fat.get(cluster as usize).copied() else {
            return Err(FsError::InvalidInput);
        };
        if value >= FAT32_EOC {
            return Ok(None);
        }
        if value == 0 {
            return Ok(None);
        }
        Ok(Some(value))
    }

    fn collect_chain(&self, start_cluster: u32) -> Result<Vec<u32>, FsError> {
        if start_cluster == 0 {
            return Ok(Vec::new());
        }
        let mut chain = Vec::new();
        let mut current = start_cluster;
        let mut remaining = self.cluster_count.saturating_add(2);
        while remaining > 0 {
            chain.push(current);
            remaining -= 1;
            match self.next_cluster(current)? {
                Some(next) => current = next,
                None => return Ok(chain),
            }
        }
        Err(FsError::Io)
    }

    fn read_file(&self, node: &Fat32Node, offset: usize, out: &mut [u8]) -> Result<usize, FsError> {
        if node.is_dir {
            return Err(FsError::IsDirectory);
        }
        if out.is_empty() || offset >= node.size as usize {
            return Ok(0);
        }
        if node.cluster == 0 {
            return Ok(0);
        }

        let chain = self.collect_chain(node.cluster)?;
        let cluster_size = self.bpb.cluster_size();
        let mut file_offset = 0usize;
        let mut written = 0usize;
        let to_copy = out.len().min(node.size as usize - offset);

        for cluster in chain {
            if written >= to_copy {
                break;
            }
            let data = self.read_cluster(cluster)?;
            let cluster_end = file_offset.saturating_add(cluster_size);
            if offset >= cluster_end {
                file_offset = cluster_end;
                continue;
            }
            let start = offset.saturating_sub(file_offset);
            let end = (start + (to_copy - written)).min(cluster_size);
            let count = end.saturating_sub(start);
            out[written..written + count].copy_from_slice(&data[start..start + count]);
            written += count;
            file_offset = cluster_end;
        }

        Ok(written)
    }

    fn read_directory(&self, cluster: u32) -> Result<Vec<FatDirRecord>, FsError> {
        let chain = self.collect_chain(cluster)?;
        let mut out = Vec::new();
        let mut logical_index = 0u64;
        let mut done = false;
        let mut pending_lfn = String::new();

        for cluster in chain {
            if done {
                break;
            }
            let data = self.read_cluster(cluster)?;
            let mut off = 0usize;
            while off + 32 <= data.len() {
                let entry = &data[off..off + 32];
                let first = entry[0];
                if first == 0x00 {
                    done = true;
                    break;
                }
                if first == 0xE5 {
                    pending_lfn.clear();
                    off += 32;
                    logical_index += 1;
                    continue;
                }
                let attr = entry[11];
                if attr == FAT32_ATTR_LFN {
                    let segment = parse_lfn_segment(entry)?;
                    if (entry[0] & 0x40) != 0 {
                        pending_lfn.clear();
                    }
                    pending_lfn.insert_str(0, segment.as_str());
                    off += 32;
                    logical_index += 1;
                    continue;
                }
                if (attr & FAT32_ATTR_VOLUME_ID) != 0 {
                    pending_lfn.clear();
                    off += 32;
                    logical_index += 1;
                    continue;
                }

                let mut name = parse_short_name(&entry[0..11]).ok_or(FsError::InvalidInput)?;
                if !pending_lfn.is_empty() {
                    name = pending_lfn.clone();
                    pending_lfn.clear();
                }
                let child_cluster =
                    ((le_u16(&entry[20..22]) as u32) << 16) | le_u16(&entry[26..28]) as u32;
                let size = le_u32(&entry[28..32]) as u64;
                let is_dir = (attr & FAT32_ATTR_DIRECTORY) != 0;

                out.push(FatDirRecord {
                    ino: ((cluster as u64) << 32) | logical_index,
                    name,
                    cluster: child_cluster,
                    size,
                    is_dir,
                });

                off += 32;
                logical_index += 1;
            }
        }

        Ok(out)
    }

    fn lookup_child(&self, dir: &Fat32Node, name: &str) -> Result<Fat32Node, FsError> {
        if !dir.is_dir {
            return Err(FsError::NotDirectory);
        }
        if name == "." {
            return Ok(dir.clone());
        }
        for entry in self.read_directory(dir.cluster)? {
            if entry.name.eq_ignore_ascii_case(name) {
                return Ok(Fat32Node {
                    ino: entry.ino,
                    cluster: entry.cluster,
                    size: entry.size,
                    is_dir: entry.is_dir,
                });
            }
        }
        Err(FsError::NotFound)
    }
}

impl FileSystem for Fat32FileSystem {
    fn name(&self) -> &str {
        "fat32"
    }

    fn root(&self) -> Arc<dyn Inode> {
        Arc::new(Fat32Inode {
            fs: Arc::new(self.clone()),
            node: Fat32Node {
                ino: 1,
                cluster: self.bpb.root_cluster,
                size: 0,
                is_dir: true,
            },
        })
    }

    fn sync(&self) -> Result<(), FsError> {
        Ok(())
    }
}

impl Inode for Fat32Inode {
    fn metadata(&self) -> Result<Metadata, FsError> {
        Ok(Metadata {
            ino: self.node.ino,
            file_type: if self.node.is_dir {
                FileType::Directory
            } else {
                FileType::Regular
            },
            mode: if self.node.is_dir { 0o040555 } else { 0o100444 },
            size: self.node.size,
            nlink: if self.node.is_dir { 2 } else { 1 },
        })
    }

    fn lookup(&self, name: &str) -> Result<Arc<dyn Inode>, FsError> {
        let node = self.fs.lookup_child(&self.node, name)?;
        Ok(Arc::new(Self {
            fs: self.fs.clone(),
            node,
        }))
    }

    fn readdir(&self) -> Result<Vec<DirEntry>, FsError> {
        if !self.node.is_dir {
            return Err(FsError::NotDirectory);
        }
        let mut out = Vec::new();
        for entry in self.fs.read_directory(self.node.cluster)? {
            out.push(DirEntry {
                ino: entry.ino,
                file_type: if entry.is_dir {
                    FileType::Directory
                } else {
                    FileType::Regular
                },
                name: entry.name,
            });
        }
        Ok(out)
    }

    fn read_at(&self, offset: usize, out: &mut [u8]) -> Result<usize, FsError> {
        self.fs.read_file(&self.node, offset, out)
    }
}

fn parse_short_name(raw: &[u8]) -> Option<String> {
    if raw.len() != 11 {
        return None;
    }
    let base = trim_ascii_spaces(&raw[0..8]);
    if base.is_empty() {
        return None;
    }
    let ext = trim_ascii_spaces(&raw[8..11]);
    if ext.is_empty() {
        Some(base.to_string())
    } else {
        Some(format!("{}.{}", base, ext))
    }
}

fn parse_lfn_segment(entry: &[u8]) -> Result<String, FsError> {
    if entry.len() != 32 {
        return Err(FsError::InvalidInput);
    }

    let mut units = Vec::with_capacity(13);
    collect_lfn_units(&mut units, &entry[1..11]);
    collect_lfn_units(&mut units, &entry[14..26]);
    collect_lfn_units(&mut units, &entry[28..32]);

    let mut out = String::new();
    for decoded in char::decode_utf16(units.into_iter()) {
        let ch = decoded.map_err(|_| FsError::InvalidInput)?;
        out.push(ch);
    }
    Ok(out)
}

fn collect_lfn_units(out: &mut Vec<u16>, raw: &[u8]) {
    let mut off = 0usize;
    while off + 2 <= raw.len() {
        let unit = le_u16(&raw[off..off + 2]);
        if unit == 0x0000 || unit == 0xFFFF {
            break;
        }
        out.push(unit);
        off += 2;
    }
}

fn trim_ascii_spaces(raw: &[u8]) -> &str {
    let end = raw
        .iter()
        .rposition(|b| *b != b' ')
        .map(|idx| idx + 1)
        .unwrap_or(0);
    core::str::from_utf8(&raw[..end]).unwrap_or("")
}

fn le_u16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[0], bytes[1]])
}

fn le_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}
