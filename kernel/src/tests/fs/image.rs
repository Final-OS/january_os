use crate::drivers::block::{BlockDevice, BlockError};
use crate::sync::Mutex;

use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

pub const MOCK_BLOCK_SIZE: usize = 512;
pub const EXT4_FS_BLOCK_SIZE: usize = 4096;

pub struct MemBlockDevice {
    name: &'static str,
    block_count: u64,
    data: Mutex<Vec<u8>>,
}

impl MemBlockDevice {
    pub fn new(name: &'static str, block_count: u64) -> Result<Self, &'static str> {
        let total_bytes = (block_count as usize)
            .checked_mul(MOCK_BLOCK_SIZE)
            .ok_or("mock device size overflow")?;
        Ok(Self {
            name,
            block_count,
            data: Mutex::new(vec![0u8; total_bytes]),
        })
    }

    pub fn write_block_raw(&self, lba: u64, buf: &[u8]) -> Result<(), &'static str> {
        if buf.len() != MOCK_BLOCK_SIZE {
            return Err("invalid mock block size");
        }
        let off = self.block_offset(lba)?;
        let mut data = self.data.lock();
        data[off..off + MOCK_BLOCK_SIZE].copy_from_slice(buf);
        Ok(())
    }

    pub fn write_bytes(&self, offset: usize, buf: &[u8]) -> Result<(), &'static str> {
        let end = offset.checked_add(buf.len()).ok_or("write overflow")?;
        let mut data = self.data.lock();
        if end > data.len() {
            return Err("write past end");
        }
        data[offset..end].copy_from_slice(buf);
        Ok(())
    }

    fn block_offset(&self, lba: u64) -> Result<usize, &'static str> {
        if lba >= self.block_count {
            return Err("mock lba out of range");
        }
        (lba as usize)
            .checked_mul(MOCK_BLOCK_SIZE)
            .ok_or("mock offset overflow")
    }
}

impl BlockDevice for MemBlockDevice {
    fn block_size(&self) -> u32 {
        MOCK_BLOCK_SIZE as u32
    }

    fn block_count(&self) -> u64 {
        self.block_count
    }

    fn name(&self) -> &str {
        self.name
    }

    fn read_block(&self, lba: u64, buf: &mut [u8]) -> Result<(), BlockError> {
        if buf.len() != MOCK_BLOCK_SIZE {
            return Err(BlockError::InvalidBufferSize);
        }
        let off = self.block_offset(lba).map_err(|_| BlockError::InvalidAddress)?;
        let data = self.data.lock();
        buf.copy_from_slice(&data[off..off + MOCK_BLOCK_SIZE]);
        Ok(())
    }

    fn write_block(&self, lba: u64, buf: &[u8]) -> Result<(), BlockError> {
        self.write_block_raw(lba, buf).map_err(|_| BlockError::IoError)
    }
}

pub fn build_fat32_device() -> Result<Arc<dyn BlockDevice>, &'static str> {
    let dev = Arc::new(MemBlockDevice::new("fat32-mock", 64)?);
    let elf = minimal_elf_image();
    let long_name = "LONG-FILE.TXT";
    let long_alias = *b"LONGFI~1TXT";

    let mut boot = [0u8; MOCK_BLOCK_SIZE];
    boot[0] = 0xEB;
    boot[1] = 0x58;
    boot[2] = 0x90;
    boot[3..11].copy_from_slice(b"JANFAT32");
    put_le16(&mut boot[11..13], 512);
    boot[13] = 1;
    put_le16(&mut boot[14..16], 1);
    boot[16] = 1;
    put_le16(&mut boot[17..19], 0);
    put_le16(&mut boot[19..21], 0);
    boot[21] = 0xF8;
    put_le16(&mut boot[22..24], 0);
    put_le16(&mut boot[24..26], 1);
    put_le16(&mut boot[26..28], 1);
    put_le32(&mut boot[28..32], 0);
    put_le32(&mut boot[32..36], 64);
    put_le32(&mut boot[36..40], 1);
    put_le16(&mut boot[40..42], 0);
    put_le16(&mut boot[42..44], 0);
    put_le32(&mut boot[44..48], 2);
    boot[510] = 0x55;
    boot[511] = 0xAA;
    dev.write_block_raw(0, &boot)?;

    let mut fat = [0u8; MOCK_BLOCK_SIZE];
    put_le32(&mut fat[0..4], 0x0FFF_FFF8);
    put_le32(&mut fat[4..8], 0xFFFF_FFFF);
    for cluster in 2..=7u32 {
        let off = cluster as usize * 4;
        put_le32(&mut fat[off..off + 4], 0x0FFF_FFFF);
    }
    dev.write_block_raw(1, &fat)?;

    let mut root = [0u8; MOCK_BLOCK_SIZE];
    write_fat_dirent(&mut root[0..32], b"HELLO   TXT", 3, b"hello from fat32\n", false);
    write_fat_dirent(&mut root[32..64], b"SUBDIR     ", 4, &[], true);
    write_fat_dirent(&mut root[64..96], b"APP     ELF", 6, elf.as_slice(), false);
    write_fat_lfn_dirents(
        &mut root[96..160],
        long_name,
        &long_alias,
        7,
        b"long fat32 name\n",
        false,
    )?;
    dev.write_block_raw(2, &root)?;

    let mut hello = [0u8; MOCK_BLOCK_SIZE];
    hello[..17].copy_from_slice(b"hello from fat32\n");
    dev.write_block_raw(3, &hello)?;

    let mut subdir = [0u8; MOCK_BLOCK_SIZE];
    write_fat_dirent(&mut subdir[0..32], b".          ", 4, &[], true);
    write_fat_dirent(&mut subdir[32..64], b"..         ", 2, &[], true);
    write_fat_dirent(&mut subdir[64..96], b"NEST    TXT", 5, b"nested fat32\n", false);
    dev.write_block_raw(4, &subdir)?;

    let mut nested = [0u8; MOCK_BLOCK_SIZE];
    nested[..13].copy_from_slice(b"nested fat32\n");
    dev.write_block_raw(5, &nested)?;

    let mut app = [0u8; MOCK_BLOCK_SIZE];
    app[..elf.len()].copy_from_slice(&elf);
    dev.write_block_raw(6, &app)?;

    let mut long_file = [0u8; MOCK_BLOCK_SIZE];
    long_file[..16].copy_from_slice(b"long fat32 name\n");
    dev.write_block_raw(7, &long_file)?;

    Ok(dev)
}

pub fn build_ext4_device() -> Result<Arc<dyn BlockDevice>, &'static str> {
    let dev = Arc::new(MemBlockDevice::new("ext4-mock", 512)?);
    let hello = b"hello from ext4\n";
    let elf = minimal_elf_image();
    let deep = build_deep_ext4_payload();

    let mut superblock = [0u8; 1024];
    put_le32(&mut superblock[0..4], 16);
    put_le32(&mut superblock[4..8], 64);
    put_le32(&mut superblock[20..24], 1);
    put_le32(&mut superblock[24..28], 2);
    put_le32(&mut superblock[32..36], 64);
    put_le32(&mut superblock[40..44], 16);
    put_le16(&mut superblock[56..58], 0xEF53);
    put_le32(&mut superblock[76..80], 1);
    put_le16(&mut superblock[88..90], 256);
    put_le16(&mut superblock[254..256], 32);
    dev.write_bytes(1024, &superblock)?;

    let mut gdt = [0u8; EXT4_FS_BLOCK_SIZE];
    put_le32(&mut gdt[0..4], 2);
    put_le32(&mut gdt[4..8], 3);
    put_le32(&mut gdt[8..12], 4);
    dev.write_bytes(EXT4_FS_BLOCK_SIZE, &gdt)?;

    let mut root_inode = [0u8; 256];
    put_le16(&mut root_inode[0..2], 0x41ED);
    put_le32(&mut root_inode[4..8], (EXT4_FS_BLOCK_SIZE * 2) as u32);
    put_le16(&mut root_inode[26..28], 2);
    put_le32(&mut root_inode[28..32], 8);
    put_le32(&mut root_inode[32..36], 0x0008_0000);
    write_extent_header(&mut root_inode[40..100], 5, 2);

    let mut hello_inode = [0u8; 256];
    put_le16(&mut hello_inode[0..2], 0x81A4);
    put_le32(&mut hello_inode[4..8], hello.len() as u32);
    put_le16(&mut hello_inode[26..28], 1);
    put_le32(&mut hello_inode[28..32], 8);
    put_le32(&mut hello_inode[32..36], 0x0008_0000);
    write_extent_header(&mut hello_inode[40..100], 7, 1);

    let mut app_inode = [0u8; 256];
    put_le16(&mut app_inode[0..2], 0x81A4);
    put_le32(&mut app_inode[4..8], elf.len() as u32);
    put_le16(&mut app_inode[26..28], 1);
    put_le32(&mut app_inode[28..32], 8);
    put_le32(&mut app_inode[32..36], 0x0008_0000);
    write_extent_header(&mut app_inode[40..100], 8, 1);

    let mut deep_inode = [0u8; 256];
    put_le16(&mut deep_inode[0..2], 0x81A4);
    put_le32(&mut deep_inode[4..8], deep.len() as u32);
    put_le16(&mut deep_inode[26..28], 1);
    put_le32(&mut deep_inode[28..32], 16);
    put_le32(&mut deep_inode[32..36], 0x0008_0000);
    write_extent_index_root(&mut deep_inode[40..100], 0, 9);

    let inode_table_base = 4 * EXT4_FS_BLOCK_SIZE;
    dev.write_bytes(inode_table_base + 256, &root_inode)?;
    dev.write_bytes(inode_table_base + (11 * 256), &hello_inode)?;
    dev.write_bytes(inode_table_base + (12 * 256), &app_inode)?;
    dev.write_bytes(inode_table_base + (13 * 256), &deep_inode)?;

    let mut root_dir = vec![0u8; EXT4_FS_BLOCK_SIZE];
    write_ext4_dirent(&mut root_dir, 0, 2, 12, b".", 2);
    write_ext4_dirent(&mut root_dir, 12, 2, 12, b"..", 2);
    dev.write_bytes(5 * EXT4_FS_BLOCK_SIZE, &root_dir)?;

    let mut root_leaf = vec![0u8; EXT4_FS_BLOCK_SIZE];
    write_ext4_dirent(&mut root_leaf, 0, 12, 20, b"hello.txt", 1);
    write_ext4_dirent(&mut root_leaf, 20, 13, 16, b"app.elf", 1);
    write_ext4_dirent(
        &mut root_leaf,
        36,
        14,
        (EXT4_FS_BLOCK_SIZE - 36) as u16,
        b"deep.txt",
        1,
    );
    dev.write_bytes(6 * EXT4_FS_BLOCK_SIZE, &root_leaf)?;

    let mut hello_block = vec![0u8; EXT4_FS_BLOCK_SIZE];
    hello_block[..hello.len()].copy_from_slice(hello);
    dev.write_bytes(7 * EXT4_FS_BLOCK_SIZE, &hello_block)?;

    let mut app_block = vec![0u8; EXT4_FS_BLOCK_SIZE];
    app_block[..elf.len()].copy_from_slice(&elf);
    dev.write_bytes(8 * EXT4_FS_BLOCK_SIZE, &app_block)?;

    let mut deep_index = vec![0u8; EXT4_FS_BLOCK_SIZE];
    write_extent_header(&mut deep_index[0..60], 10, 2);
    dev.write_bytes(9 * EXT4_FS_BLOCK_SIZE, &deep_index)?;
    dev.write_bytes(10 * EXT4_FS_BLOCK_SIZE, &deep[..EXT4_FS_BLOCK_SIZE])?;
    let mut deep_tail = vec![0u8; EXT4_FS_BLOCK_SIZE];
    let tail_len = deep.len() - EXT4_FS_BLOCK_SIZE;
    deep_tail[..tail_len].copy_from_slice(&deep[EXT4_FS_BLOCK_SIZE..]);
    dev.write_bytes(11 * EXT4_FS_BLOCK_SIZE, &deep_tail)?;

    Ok(dev)
}

pub fn minimal_elf_image() -> Vec<u8> {
    let mut elf = vec![0u8; 256];
    let elf_len = elf.len() as u64;
    elf[0..4].copy_from_slice(b"\x7fELF");
    elf[4] = 2;
    elf[5] = 1;
    elf[6] = 1;
    elf[16..18].copy_from_slice(&2u16.to_le_bytes());
    elf[18..20].copy_from_slice(&62u16.to_le_bytes());
    elf[20..24].copy_from_slice(&1u32.to_le_bytes());
    elf[24..32].copy_from_slice(&0x401080u64.to_le_bytes());
    elf[32..40].copy_from_slice(&64u64.to_le_bytes());
    elf[52..54].copy_from_slice(&64u16.to_le_bytes());
    elf[54..56].copy_from_slice(&56u16.to_le_bytes());
    elf[56..58].copy_from_slice(&1u16.to_le_bytes());

    let ph = &mut elf[64..120];
    ph[0..4].copy_from_slice(&1u32.to_le_bytes());
    ph[4..8].copy_from_slice(&5u32.to_le_bytes());
    ph[8..16].copy_from_slice(&0u64.to_le_bytes());
    ph[16..24].copy_from_slice(&0x401000u64.to_le_bytes());
    ph[32..40].copy_from_slice(&elf_len.to_le_bytes());
    ph[40..48].copy_from_slice(&elf_len.to_le_bytes());
    ph[48..56].copy_from_slice(&0x1000u64.to_le_bytes());
    elf
}

fn write_fat_dirent(slot: &mut [u8], name: &[u8; 11], cluster: u32, data: &[u8], is_dir: bool) {
    slot.fill(0);
    slot[0..11].copy_from_slice(name);
    slot[11] = if is_dir { 0x10 } else { 0x20 };
    put_le16(&mut slot[20..22], (cluster >> 16) as u16);
    put_le16(&mut slot[26..28], cluster as u16);
    put_le32(&mut slot[28..32], data.len() as u32);
}

fn write_fat_lfn_dirents(
    slots: &mut [u8],
    long_name: &str,
    alias: &[u8; 11],
    cluster: u32,
    data: &[u8],
    is_dir: bool,
) -> Result<(), &'static str> {
    let utf16: Vec<u16> = long_name.encode_utf16().collect();
    let entry_count = utf16.len().div_ceil(13);
    let needed = (entry_count + 1) * 32;
    if slots.len() < needed {
        return Err("lfn slots too small");
    }

    for idx in 0..entry_count {
        let entry_off = idx * 32;
        let entry = &mut slots[entry_off..entry_off + 32];
        entry.fill(0xFF);
        let order = (entry_count - idx) as u8;
        entry[0] = if idx == 0 { order | 0x40 } else { order };
        entry[11] = 0x0F;
        entry[12] = 0;
        entry[13] = fat_short_name_checksum(alias);
        entry[26] = 0;
        entry[27] = 0;

        let start = (entry_count - idx - 1) * 13;
        let end = (start + 13).min(utf16.len());
        let chunk = &utf16[start..end];
        write_lfn_chunk(entry, chunk);
    }

    write_fat_dirent(&mut slots[entry_count * 32..entry_count * 32 + 32], alias, cluster, data, is_dir);
    Ok(())
}

fn write_lfn_chunk(entry: &mut [u8], chunk: &[u16]) {
    let positions = [1usize, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30];
    for (idx, pos) in positions.iter().enumerate() {
        let value = if idx < chunk.len() {
            chunk[idx]
        } else if idx == chunk.len() {
            0x0000
        } else {
            0xFFFF
        };
        put_le16(&mut entry[*pos..*pos + 2], value);
    }
}

fn fat_short_name_checksum(alias: &[u8; 11]) -> u8 {
    let mut sum = 0u8;
    for byte in alias {
        sum = ((sum & 1) << 7).wrapping_add(sum >> 1).wrapping_add(*byte);
    }
    sum
}

fn write_extent_header(slot: &mut [u8], start_block: u32, block_len: u16) {
    put_le16(&mut slot[0..2], 0xF30A);
    put_le16(&mut slot[2..4], 1);
    put_le16(&mut slot[4..6], 4);
    put_le16(&mut slot[6..8], 0);
    put_le32(&mut slot[12..16], 0);
    put_le16(&mut slot[16..18], block_len);
    put_le16(&mut slot[18..20], 0);
    put_le32(&mut slot[20..24], start_block);
}

fn write_extent_index_root(slot: &mut [u8], logical: u32, child_block: u32) {
    put_le16(&mut slot[0..2], 0xF30A);
    put_le16(&mut slot[2..4], 1);
    put_le16(&mut slot[4..6], 4);
    put_le16(&mut slot[6..8], 1);
    put_le32(&mut slot[12..16], logical);
    put_le32(&mut slot[16..20], child_block);
    put_le16(&mut slot[20..22], 0);
    put_le16(&mut slot[22..24], 0);
}

pub fn build_deep_ext4_payload() -> Vec<u8> {
    let mut out = vec![0u8; EXT4_FS_BLOCK_SIZE + 321];
    let pattern = b"deep extent ext4 payload ";
    let mut off = 0usize;
    while off < out.len() {
        let count = (out.len() - off).min(pattern.len());
        out[off..off + count].copy_from_slice(&pattern[..count]);
        off += count;
    }
    out
}

fn write_ext4_dirent(buf: &mut [u8], offset: usize, ino: u32, rec_len: u16, name: &[u8], ty: u8) {
    put_le32(&mut buf[offset..offset + 4], ino);
    put_le16(&mut buf[offset + 4..offset + 6], rec_len);
    buf[offset + 6] = name.len() as u8;
    buf[offset + 7] = ty;
    buf[offset + 8..offset + 8 + name.len()].copy_from_slice(name);
}

fn put_le16(dst: &mut [u8], value: u16) {
    dst.copy_from_slice(&value.to_le_bytes());
}

fn put_le32(dst: &mut [u8], value: u32) {
    dst.copy_from_slice(&value.to_le_bytes());
}
