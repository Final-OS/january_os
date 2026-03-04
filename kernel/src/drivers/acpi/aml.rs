//! Simple AML Parser for finding _S5_ package
//!
//! This is a minimal implementation to support ACPI shutdown.
//! It does not support full AML parsing.

use crate::{debug, warn};

/// ACPI Shutdown values
#[derive(Debug, Clone, Copy)]
pub struct AcpiS5State {
    pub pm1a_cnt_val: u16,
    pub pm1b_cnt_val: u16,
}

/// Parse DSDT to find _S5_ package
pub unsafe fn parse_s5(dsdt_addr: u64, dsdt_len: usize) -> Option<AcpiS5State> {
    let dsdt_ptr = (dsdt_addr + crate::mm::direct_map_offset()) as *const u8;
    let dsdt_data = core::slice::from_raw_parts(dsdt_ptr, dsdt_len);

    // Scan for "_S5_" signature
    // "_S5_" = [0x5F, 0x53, 0x35, 0x5F]
    for i in 0..dsdt_len.saturating_sub(4) {
        if dsdt_data[i] == 0x5F
            && dsdt_data[i + 1] == 0x53
            && dsdt_data[i + 2] == 0x35
            && dsdt_data[i + 3] == 0x5F
        {
            debug!("Found _S5_ at offset {:#x}", i);

            // Check if it is a NameOp (0x08) preceding it?
            // Usually: 08 5F 53 35 5F 12 ...
            // Or just: 5F 53 35 5F 12 ...

            // Look ahead for PackageOp (0x12)
            // It should be immediately after, or very close.
            let mut pkg_offset = i + 4;

            // Skip whitespaces or whatever (unlikely in compiled AML)
            // But let's check next byte
            if pkg_offset >= dsdt_len {
                return None;
            }

            if dsdt_data[pkg_offset] != 0x12 {
                // Maybe it's Name path segment?
                // Let's look a bit further
                debug!(
                    "Expected PackageOp (0x12) after _S5_, found {:#x}",
                    dsdt_data[pkg_offset]
                );
                continue;
            }

            // Parse Package
            // 12 PkgLength NumElements Element1 Element2 ...
            pkg_offset += 1;

            let (pkg_len, bytes_read) = parse_pkg_length(&dsdt_data[pkg_offset..])?;
            pkg_offset += bytes_read;

            if pkg_offset >= dsdt_len {
                return None;
            }

            let num_elements = dsdt_data[pkg_offset];
            pkg_offset += 1;

            debug!("_S5_ Package: len={}, elements={}", pkg_len, num_elements);

            // We need at least 2 elements (PM1a, PM1b)
            // But usually there are 4 (PM1a, PM1b, Reserved, Reserved)
            // See ACPI Spec 4.8.4.1 \_Sx states

            let mut values = [0u16; 4];
            let mut val_idx = 0;
            let pkg_end = i + 5 + pkg_len;

            while val_idx < 4 && pkg_offset < pkg_end {
                debug!(
                    "  [AML] Parsing element at {:#x} (limit {:#x})",
                    pkg_offset, pkg_end
                );
                if let Some((val, len)) = parse_integer(&dsdt_data[pkg_offset..]) {
                    debug!("  [AML] Got integer: {:#x} (len={})", val, len);
                    values[val_idx] = val as u16;
                    val_idx += 1;
                    pkg_offset += len;
                } else {
                    // Unknown opcode, abort
                    debug!(
                        "  [AML] Unknown opcode at offset {:#x}: {:#x}",
                        pkg_offset, dsdt_data[pkg_offset]
                    );
                    break;
                }
            }

            // Allow partial success (at least PM1a)
            if val_idx >= 1 {
                let pm1a = values[0];
                let pm1b = if val_idx >= 2 { values[1] } else { pm1a }; // Use PM1a for PM1b if missing

                debug!(
                    "_S5_ Values: PM1a={}, PM1b={} (found {} elements)",
                    pm1a, pm1b, val_idx
                );

                return Some(AcpiS5State {
                    pm1a_cnt_val: pm1a,
                    pm1b_cnt_val: pm1b,
                });
            } else {
                debug!("_S5_ Parsing failed: val_idx={} < 1", val_idx);
            }
        }
    }

    None
}

fn parse_pkg_length(data: &[u8]) -> Option<(usize, usize)> {
    if data.is_empty() {
        return None;
    }

    let lead = data[0];
    let byte_count = (lead >> 6) & 0x3;

    if byte_count == 0 {
        // 1 byte length
        Some(((lead & 0x3F) as usize, 1))
    } else {
        // Multibyte
        // lead bits 0-3 are lowest 4 bits of length
        // next bytes are higher bits
        if data.len() < (byte_count as usize + 1) {
            return None;
        }

        let mut len = (lead & 0x0F) as usize;
        for i in 0..byte_count {
            len |= (data[1 + i as usize] as usize) << (4 + i * 8);
        }

        Some((len, 1 + byte_count as usize))
    }
}

fn parse_integer(data: &[u8]) -> Option<(u64, usize)> {
    if data.is_empty() {
        return None;
    }

    match data[0] {
        0x00 => Some((0, 1)),                  // ZeroOp
        0x01 => Some((1, 1)),                  // OneOp
        0xFF => Some((0xFFFFFFFFFFFFFFFF, 1)), // OnesOp
        0x0A => {
            // BytePrefix
            if data.len() < 2 {
                return None;
            }
            Some((data[1] as u64, 2))
        }
        0x0B => {
            // WordPrefix
            if data.len() < 3 {
                return None;
            }
            let val = u16::from_le_bytes([data[1], data[2]]);
            Some((val as u64, 3))
        }
        0x0C => {
            // DWordPrefix
            if data.len() < 5 {
                return None;
            }
            let val = u32::from_le_bytes([data[1], data[2], data[3], data[4]]);
            Some((val as u64, 5))
        }
        0x0E => {
            // QWordPrefix
            if data.len() < 9 {
                return None;
            }
            let val = u64::from_le_bytes([
                data[1], data[2], data[3], data[4], data[5], data[6], data[7], data[8],
            ]);
            Some((val, 9))
        }
        _ => None,
    }
}
