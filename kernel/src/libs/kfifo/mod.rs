//! KFIFO（内核 FIFO）

use super::ring_buffer::{RingBuffer, RingBufferError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KfifoError {
    CapacityZero,
    Full,
}

impl From<RingBufferError> for KfifoError {
    fn from(value: RingBufferError) -> Self {
        match value {
            RingBufferError::CapacityZero => KfifoError::CapacityZero,
            RingBufferError::Full => KfifoError::Full,
        }
    }
}

pub struct KFifo<T> {
    inner: RingBuffer<T>,
}

impl<T> KFifo<T> {
    pub fn with_capacity(capacity: usize) -> Result<Self, KfifoError> {
        let inner = RingBuffer::with_capacity(capacity).map_err(KfifoError::from)?;
        Ok(Self { inner })
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        self.inner.is_full()
    }

    #[inline]
    pub fn avail(&self) -> usize {
        self.inner.available_read()
    }

    #[inline]
    pub fn unused(&self) -> usize {
        self.inner.available_write()
    }

    #[inline]
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    #[inline]
    pub fn reset(&mut self) {
        self.clear();
    }

    pub fn push(&mut self, value: T) -> Result<(), KfifoError> {
        self.inner.push(value).map_err(KfifoError::from)
    }

    pub fn pop(&mut self) -> Option<T> {
        self.inner.pop()
    }

    pub fn push_overwrite(&mut self, value: T) -> Option<T> {
        self.inner.push_overwrite(value)
    }

    pub fn peek(&self) -> Option<&T> {
        self.inner.peek()
    }

    pub fn peek_mut(&mut self) -> Option<&mut T> {
        self.inner.peek_mut()
    }

    pub fn pop_back(&mut self) -> Option<T> {
        self.inner.pop_back()
    }
}

impl<T: Copy> KFifo<T> {
    #[inline]
    pub fn in_slice(&mut self, source: &[T]) -> usize {
        self.inner.write_slice(source)
    }

    #[inline]
    pub fn out_slice(&mut self, output: &mut [T]) -> usize {
        self.inner.read_slice(output)
    }
}
