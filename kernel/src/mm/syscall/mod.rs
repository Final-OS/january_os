use alloc::vec::Vec;
use core::cmp;

use crate::errno::{E2BIG, EBADF, EBUSY, EINVAL, ENOMEM, ESRCH};
use crate::fs;
use crate::mm;
use crate::syscall::{SyscallArgs, SyscallRet, err, ok};
use crate::task;

const MMAP_PROT_ALLOWED: u32 =
    mm::prot_flags::PROT_READ | mm::prot_flags::PROT_WRITE | mm::prot_flags::PROT_EXEC;
const MMAP_FLAGS_ALLOWED: u32 = mm::mmap_flags::MAP_SHARED
    | mm::mmap_flags::MAP_PRIVATE
    | mm::mmap_flags::MAP_FIXED
    | mm::mmap_flags::MAP_ANONYMOUS
    | mm::mmap_flags::MAP_GROWSDOWN
    | mm::mmap_flags::MAP_LOCKED
    | mm::mmap_flags::MAP_HUGETLB;
const MMAP_FLAGS_UNSUPPORTED: u32 = mm::mmap_flags::MAP_LOCKED | mm::mmap_flags::MAP_HUGETLB;

mod addr;
mod mmap;
mod protect;
mod txn;

pub(crate) use addr::*;
pub(crate) use mmap::{sys_mmap, sys_munmap};
pub(crate) use protect::{sys_brk, sys_mprotect};
pub(crate) use txn::*;
