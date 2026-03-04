//! Block device tests

use crate::drivers::block::{self, BlockDevice};
use crate::{error, kprintln, ok, warn};

use alloc::format;

pub fn run() {
    run_with_filter(None);
}

pub fn run_with_filter(filter: Option<&str>) {
    kprintln!("=== Block Device Tests ===");
    block_step("start block test suite");
    if crate::config::DEBUG_VERBOSE {
        kprintln!("[test/block] filter={:?}", filter);
    }

    match filter {
        None | Some("all") | Some("virtio") => {
            block_step("run case=virtio");
            test_virtio_blk();
        }
        Some(name) => {
            error!("Unknown block test: {}", name);
            kprintln!("Available block tests: virtio");
        }
    }

    block_step("block test suite done");
    kprintln!();
}

fn test_virtio_blk() {
    block_step("check virtio-blk device");

    // Driver is registered during boot, devices are probed by PCI subsystem
    if let Some(dev) = block::virtio_blk::get_device() {
        pass("virtio_device_found");

        block_step("verify device properties");

        let block_size = dev.block_size();
        let block_count = dev.block_count();

        kprintln!(
            "[test/block] block_size={} block_count={}",
            block_size,
            block_count
        );

        if block_size == 512 {
            pass("virtio_block_size");
        } else {
            fail("virtio_block_size", "expected 512");
        }

        if block_count > 0 {
            pass("virtio_capacity");

            block_step("read first block");
            let mut buf = [0u8; 512];
            match dev.read_block(0, &mut buf) {
                Ok(()) => {
                    pass("virtio_read");
                    kprintln!("[test/block] first 16 bytes: {:02x?}", &buf[..16]);
                }
                Err(_) => {
                    fail("virtio_read", "read failed");
                }
            }

            if !dev.is_read_only() {
                block_step("write test (not read-only)");
                let test_data = [0xDEu8; 512];
                match dev.write_block(0, &test_data) {
                    Ok(()) => {
                        pass("virtio_write");
                    }
                    Err(_) => {
                        fail("virtio_write", "write failed");
                    }
                }
            } else {
                block_step("device is read-only, skip write test");
                pass("virtio_readonly_check");
            }
        } else {
            fail("virtio_capacity", "zero capacity");
        }
    } else {
        pass("virtio_init_skipped");
        warn!("virtio-blk device not available (this is OK if no virtio disk attached)");
    }
}

pub(super) fn pass(name: &str) {
    ok!("block/{}", name);
}

pub(super) fn fail(name: &str, msg: &str) {
    error!("block/{}: {}", name, msg);
}

fn block_step(msg: &str) {
    if crate::config::DEBUG_VERBOSE {
        kprintln!("[test/block][step] {}", msg);
    }
}
