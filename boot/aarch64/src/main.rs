//! january_os UEFI bootloader scaffold (aarch64)
//!
//! This target keeps directory and crate structure aligned with x86_64.
//! Real handoff/paging/device stages will be implemented incrementally.

#![no_std]
#![no_main]

use uefi::prelude::*;

#[entry]
fn main() -> Status {
    Status::UNSUPPORTED
}
