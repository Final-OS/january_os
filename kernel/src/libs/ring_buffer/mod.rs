//! 通用环形缓冲区

use alloc::vec::Vec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingBufferError {
    CapacityZero,
    Full,
}

pub struct RingBuffer<T> {
    slots: Vec<Option<T>>,
    read_index: usize,
    write_index: usize,
    len: usize,
}

impl<T> RingBuffer<T> {
    pub fn with_capacity(capacity: usize) -> Result<Self, RingBufferError> {
        if capacity == 0 {
            return Err(RingBufferError::CapacityZero);
        }

        let mut slots = Vec::with_capacity(capacity);
        slots.resize_with(capacity, || None);

        Ok(Self {
            slots,
            read_index: 0,
            write_index: 0,
            len: 0,
        })
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        self.len == self.capacity()
    }

    #[inline]
    pub fn available_read(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn available_write(&self) -> usize {
        self.capacity().saturating_sub(self.len)
    }

    #[inline]
    pub fn is_half_full(&self) -> bool {
        self.len * 2 >= self.capacity()
    }

    pub fn clear(&mut self) {
        while self.pop().is_some() {}
        self.read_index = 0;
        self.write_index = 0;
    }

    pub fn push(&mut self, value: T) -> Result<(), RingBufferError> {
        if self.is_full() {
            return Err(RingBufferError::Full);
        }

        self.slots[self.write_index] = Some(value);
        self.write_index = (self.write_index + 1) % self.capacity();
        self.len += 1;
        Ok(())
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }

        let value = self.slots[self.read_index].take();
        self.read_index = (self.read_index + 1) % self.capacity();
        self.len -= 1;
        value
    }

    pub fn peek(&self) -> Option<&T> {
        if self.is_empty() {
            None
        } else {
            self.slots[self.read_index].as_ref()
        }
    }

    pub fn peek_mut(&mut self) -> Option<&mut T> {
        if self.is_empty() {
            None
        } else {
            self.slots[self.read_index].as_mut()
        }
    }

    pub fn pop_back(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }

        self.write_index = (self.write_index + self.capacity() - 1) % self.capacity();
        let value = self.slots[self.write_index].take();
        self.len -= 1;
        value
    }

    pub fn push_overwrite(&mut self, value: T) -> Option<T> {
        if self.is_full() {
            let dropped = self.pop();
            let _ = self.push(value);
            dropped
        } else {
            let _ = self.push(value);
            None
        }
    }

    pub fn as_slices(&self) -> (&[Option<T>], &[Option<T>]) {
        let cap = self.capacity();
        if self.len == 0 {
            return (&[], &[]);
        }

        let first_len = core::cmp::min(self.len, cap - self.read_index);
        let second_len = self.len - first_len;

        let first = &self.slots[self.read_index..self.read_index + first_len];
        let second = if second_len == 0 {
            &[]
        } else {
            &self.slots[..second_len]
        };

        (first, second)
    }
}

impl<T: Copy> RingBuffer<T> {
    pub fn write_slice(&mut self, source: &[T]) -> usize {
        let mut written = 0;
        for &item in source {
            if self.push(item).is_err() {
                break;
            }
            written += 1;
        }
        written
    }

    pub fn read_slice(&mut self, output: &mut [T]) -> usize {
        let mut read = 0;
        for slot in output.iter_mut() {
            let Some(value) = self.pop() else {
                break;
            };
            *slot = value;
            read += 1;
        }
        read
    }
}
