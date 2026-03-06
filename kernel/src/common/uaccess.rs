use alloc::string::String;
use alloc::vec::Vec;

use crate::errno::{EFAULT, EINVAL, ENAMETOOLONG, ENOENT};
use crate::mm;

pub(crate) fn validate_user_range(ptr: usize, size: usize) -> Result<(), i32> {
    if size == 0 {
        return Ok(());
    }

    let last = ptr.checked_add(size.saturating_sub(1)).ok_or(EFAULT)?;
    let start = ptr as u64;
    let end = last as u64;

    if start < mm::USER_SPACE_START || end < mm::USER_SPACE_START {
        return Err(EFAULT);
    }
    if !mm::is_user_addr(start) || !mm::is_user_addr(end) {
        return Err(EFAULT);
    }

    Ok(())
}

#[inline]
pub(crate) fn read_user_struct<T: Copy>(ptr: usize) -> Result<T, i32> {
    validate_user_range(ptr, core::mem::size_of::<T>())?;
    Ok(unsafe { core::ptr::read(ptr as *const T) })
}

#[inline]
pub(crate) fn write_user_struct<T: Copy>(ptr: usize, value: &T) -> Result<(), i32> {
    validate_user_range(ptr, core::mem::size_of::<T>())?;
    unsafe {
        core::ptr::write(ptr as *mut T, *value);
    }
    Ok(())
}

#[inline]
pub(crate) fn read_user_usize(ptr: usize) -> Result<usize, i32> {
    validate_user_range(ptr, core::mem::size_of::<usize>())?;
    Ok(unsafe { core::ptr::read(ptr as *const usize) })
}

#[inline]
pub(crate) fn read_user_u8(ptr: usize) -> Result<u8, i32> {
    validate_user_range(ptr, 1)?;
    Ok(unsafe { core::ptr::read(ptr as *const u8) })
}

#[inline]
pub(crate) fn read_user_u64(ptr: usize) -> Result<u64, i32> {
    validate_user_range(ptr, core::mem::size_of::<u64>())?;
    Ok(unsafe { core::ptr::read(ptr as *const u64) })
}

#[inline]
pub(crate) fn write_user_u64(ptr: usize, value: u64) -> Result<(), i32> {
    validate_user_range(ptr, core::mem::size_of::<u64>())?;
    unsafe {
        core::ptr::write(ptr as *mut u64, value);
    }
    Ok(())
}

pub(crate) unsafe fn read_user_cstring(ptr: usize, max_len: usize) -> Result<String, i32> {
    if ptr == 0 {
        return Err(EFAULT);
    }

    let mut bytes: Vec<u8> = Vec::new();

    for index in 0..max_len {
        let cur = ptr.checked_add(index).ok_or(EFAULT)?;
        validate_user_range(cur, 1)?;

        let value = unsafe { core::ptr::read(cur as *const u8) };
        if value == 0 {
            if bytes.is_empty() {
                return Err(ENOENT);
            }
            let text = core::str::from_utf8(&bytes).map_err(|_| EINVAL)?;
            return Ok(String::from(text));
        }

        bytes.push(value);
    }

    Err(ENAMETOOLONG)
}
