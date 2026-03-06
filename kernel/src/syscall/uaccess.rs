use crate::error::{KernelError, KernelResult};

pub fn ensure_user_ptr<T>(ptr: *const T) -> KernelResult<*const T> {
    if ptr.is_null() {
        return Err(KernelError::InvalidAddress);
    }
    Ok(ptr)
}
