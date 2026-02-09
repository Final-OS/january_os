//! 通用 ID 分配器

use crate::libs::bitmap::Bitmap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdAllocError {
    InvalidRange,
    Exhausted,
    OutOfRange,
    AlreadyAllocated,
    NotAllocated,
}

pub struct IdAllocator {
    base: usize,
    bitmap: Bitmap,
}

impl IdAllocator {
    pub fn new(base: usize, count: usize) -> Result<Self, IdAllocError> {
        if count == 0 {
            return Err(IdAllocError::InvalidRange);
        }

        Ok(Self {
            base,
            bitmap: Bitmap::new(count),
        })
    }

    #[inline]
    pub fn base(&self) -> usize {
        self.base
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.bitmap.nbits()
    }

    #[inline]
    pub fn allocated(&self) -> usize {
        self.bitmap.count_ones()
    }

    #[inline]
    pub fn available(&self) -> usize {
        self.capacity().saturating_sub(self.allocated())
    }

    pub fn alloc(&mut self) -> Result<usize, IdAllocError> {
        let Some(index) = self.bitmap.find_first_zero() else {
            return Err(IdAllocError::Exhausted);
        };

        self.bitmap.set(index);
        Ok(self.base + index)
    }

    pub fn alloc_specific(&mut self, id: usize) -> Result<(), IdAllocError> {
        let index = self.index_of(id)?;
        if self.bitmap.test(index) {
            return Err(IdAllocError::AlreadyAllocated);
        }

        self.bitmap.set(index);
        Ok(())
    }

    pub fn free(&mut self, id: usize) -> Result<(), IdAllocError> {
        let index = self.index_of(id)?;
        if !self.bitmap.test(index) {
            return Err(IdAllocError::NotAllocated);
        }

        self.bitmap.clear(index);
        Ok(())
    }

    pub fn alloc_range(&mut self, count: usize) -> Result<usize, IdAllocError> {
        if count == 0 || count > self.capacity() {
            return Err(IdAllocError::InvalidRange);
        }

        let capacity = self.capacity();
        let mut run = 0usize;
        let mut start = 0usize;

        for index in 0..capacity {
            if !self.bitmap.test(index) {
                if run == 0 {
                    start = index;
                }
                run += 1;

                if run == count {
                    for mark in start..(start + count) {
                        self.bitmap.set(mark);
                    }
                    return Ok(self.base + start);
                }
            } else {
                run = 0;
            }
        }

        Err(IdAllocError::Exhausted)
    }

    pub fn free_range(&mut self, start_id: usize, count: usize) -> Result<(), IdAllocError> {
        if count == 0 {
            return Err(IdAllocError::InvalidRange);
        }

        let start = self.index_of(start_id)?;
        let end = start
            .checked_add(count)
            .filter(|&value| value <= self.capacity())
            .ok_or(IdAllocError::OutOfRange)?;

        for index in start..end {
            if !self.bitmap.test(index) {
                return Err(IdAllocError::NotAllocated);
            }
        }

        for index in start..end {
            self.bitmap.clear(index);
        }

        Ok(())
    }

    pub fn first_allocated(&self) -> Option<usize> {
        self.bitmap.find_first_set().map(|index| self.base + index)
    }

    pub fn first_free(&self) -> Option<usize> {
        self.bitmap.find_first_zero().map(|index| self.base + index)
    }

    pub fn is_allocated(&self, id: usize) -> bool {
        self.index_of(id)
            .map(|index| self.bitmap.test(index))
            .unwrap_or(false)
    }

    pub fn clear(&mut self) {
        self.bitmap.clear_all();
    }

    fn index_of(&self, id: usize) -> Result<usize, IdAllocError> {
        if id < self.base {
            return Err(IdAllocError::OutOfRange);
        }

        let index = id - self.base;
        if index >= self.capacity() {
            return Err(IdAllocError::OutOfRange);
        }

        Ok(index)
    }
}
