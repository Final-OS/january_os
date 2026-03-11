use super::*;
use crate::common::uaccess::{
    read_user_u8 as shared_read_user_u8, read_user_usize as shared_read_user_usize,
    validate_user_range,
};

pub(crate) fn validate_read_ptr(ptr: usize, size: usize, _align: usize) -> Result<(), i32> {
    validate_user_range(ptr, size)
}

pub(crate) fn read_user_u8(ptr: usize) -> Result<u8, i32> {
    validate_read_ptr(ptr, core::mem::size_of::<u8>(), core::mem::align_of::<u8>())?;
    shared_read_user_u8(ptr)
}

pub(crate) fn read_user_usize(ptr: usize) -> Result<usize, i32> {
    validate_read_ptr(
        ptr,
        core::mem::size_of::<usize>(),
        core::mem::align_of::<usize>(),
    )?;
    shared_read_user_usize(ptr)
}

pub(crate) fn read_user_cstring(ptr: usize, max_len: usize) -> Result<String, i32> {
    if ptr == 0 {
        return Err(EFAULT);
    }

    let mut bytes: Vec<u8> = Vec::new();
    for offset in 0..max_len {
        let addr = ptr.checked_add(offset).ok_or(EFAULT)?;
        let value = read_user_u8(addr)?;
        if value == 0 {
            let string = core::str::from_utf8(bytes.as_slice()).map_err(|_| EINVAL)?;
            return Ok(String::from(string));
        }
        bytes.push(value);
    }

    Err(ENAMETOOLONG)
}

pub(crate) fn read_user_string_array(
    list_ptr: usize,
    max_items: usize,
    item_len_limit: usize,
) -> Result<(Vec<String>, usize), i32> {
    if list_ptr == 0 {
        return Ok((Vec::new(), 0));
    }

    let mut result: Vec<String> = Vec::new();
    let mut used_bytes = 0usize;
    let ptr_size = core::mem::size_of::<usize>();

    for index in 0..max_items {
        let index_offset = index.checked_mul(ptr_size).ok_or(E2BIG)?;
        let entry_ptr = list_ptr.checked_add(index_offset).ok_or(EFAULT)?;
        let value_ptr = read_user_usize(entry_ptr)?;

        if value_ptr == 0 {
            return Ok((result, used_bytes));
        }

        let value = read_user_cstring(value_ptr, item_len_limit)?;
        used_bytes = used_bytes
            .checked_add(value.len().saturating_add(1))
            .ok_or(E2BIG)?;
        if used_bytes > EXEC_TOTAL_BYTES_MAX {
            return Err(E2BIG);
        }
        result.push(value);
    }

    Err(E2BIG)
}

pub(crate) fn parse_execve_payload(
    path_ptr: usize,
    argv_ptr: usize,
    envp_ptr: usize,
) -> Result<(String, Vec<String>, Vec<String>), i32> {
    let path = read_user_cstring(path_ptr, EXEC_PATH_MAX)?;
    let (argv, argv_bytes) = read_user_string_array(argv_ptr, EXEC_ARG_LIST_MAX, EXEC_ARG_STR_MAX)?;
    let (envp, envp_bytes) = read_user_string_array(envp_ptr, EXEC_ENV_LIST_MAX, EXEC_ARG_STR_MAX)?;

    let total_bytes = path
        .len()
        .saturating_add(1)
        .saturating_add(argv_bytes)
        .saturating_add(envp_bytes);
    if total_bytes > EXEC_TOTAL_BYTES_MAX {
        return Err(E2BIG);
    }

    Ok((path, argv, envp))
}
