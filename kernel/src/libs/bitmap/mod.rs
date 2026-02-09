//! 位图工具

use alloc::vec;
use alloc::vec::Vec;

pub struct Bitmap {
    bits: Vec<usize>,
    nbits: usize,
}

impl Bitmap {
    pub fn new(nbits: usize) -> Self {
        let word_bits = usize::BITS as usize;
        let words = if nbits == 0 {
            0
        } else {
            (nbits + word_bits - 1) / word_bits
        };
        Self {
            bits: vec![0; words],
            nbits,
        }
    }

    #[inline]
    pub fn nbits(&self) -> usize {
        self.nbits
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.nbits == 0
    }

    pub fn clear_all(&mut self) {
        for word in self.bits.iter_mut() {
            *word = 0;
        }
    }

    pub fn set_all(&mut self) {
        for word in self.bits.iter_mut() {
            *word = usize::MAX;
        }
        self.mask_tail_bits();
    }

    pub fn set(&mut self, bit: usize) -> bool {
        let Some((word, mask)) = self.word_mask(bit) else {
            return false;
        };
        self.bits[word] |= mask;
        true
    }

    pub fn clear(&mut self, bit: usize) -> bool {
        let Some((word, mask)) = self.word_mask(bit) else {
            return false;
        };
        self.bits[word] &= !mask;
        true
    }

    pub fn test(&self, bit: usize) -> bool {
        let Some((word, mask)) = self.word_mask(bit) else {
            return false;
        };
        (self.bits[word] & mask) != 0
    }

    pub fn find_first_set(&self) -> Option<usize> {
        for (word_index, &word) in self.bits.iter().enumerate() {
            if word == 0 {
                continue;
            }
            let offset = word.trailing_zeros() as usize;
            let bit = word_index * (usize::BITS as usize) + offset;
            if bit < self.nbits {
                return Some(bit);
            }
        }
        None
    }

    pub fn find_first_zero(&self) -> Option<usize> {
        let word_bits = usize::BITS as usize;
        for (word_index, &word) in self.bits.iter().enumerate() {
            if word == usize::MAX {
                continue;
            }
            let offset = (!word).trailing_zeros() as usize;
            let bit = word_index * word_bits + offset;
            if bit < self.nbits {
                return Some(bit);
            }
        }
        None
    }

    pub fn count_ones(&self) -> usize {
        self.bits
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum()
    }

    #[inline]
    pub fn count_zeros(&self) -> usize {
        self.nbits.saturating_sub(self.count_ones())
    }

    pub fn set_range(&mut self, start: usize, len: usize) -> bool {
        self.update_range(start, len, true)
    }

    pub fn clear_range(&mut self, start: usize, len: usize) -> bool {
        self.update_range(start, len, false)
    }

    pub fn test_all_set(&self, start: usize, len: usize) -> bool {
        if !self.valid_range(start, len) {
            return false;
        }

        for bit in start..(start + len) {
            if !self.test(bit) {
                return false;
            }
        }
        true
    }

    pub fn test_all_clear(&self, start: usize, len: usize) -> bool {
        if !self.valid_range(start, len) {
            return false;
        }

        for bit in start..(start + len) {
            if self.test(bit) {
                return false;
            }
        }
        true
    }

    pub fn find_next_set(&self, from: usize) -> Option<usize> {
        if from >= self.nbits {
            return None;
        }

        for bit in from..self.nbits {
            if self.test(bit) {
                return Some(bit);
            }
        }
        None
    }

    pub fn find_next_zero(&self, from: usize) -> Option<usize> {
        if from >= self.nbits {
            return None;
        }

        for bit in from..self.nbits {
            if !self.test(bit) {
                return Some(bit);
            }
        }
        None
    }

    pub fn find_contiguous_zeros(&self, len: usize) -> Option<usize> {
        self.find_contiguous_zeros_from(0, len)
    }

    pub fn find_contiguous_zeros_from(&self, from: usize, len: usize) -> Option<usize> {
        if len == 0 || self.nbits == 0 || from >= self.nbits {
            return None;
        }

        let mut run = 0usize;
        let mut run_start = from;

        for bit in from..self.nbits {
            if !self.test(bit) {
                if run == 0 {
                    run_start = bit;
                }
                run += 1;
                if run == len {
                    return Some(run_start);
                }
            } else {
                run = 0;
            }
        }

        None
    }

    fn update_range(&mut self, start: usize, len: usize, set: bool) -> bool {
        if !self.valid_range(start, len) {
            return false;
        }

        for bit in start..(start + len) {
            if set {
                self.set(bit);
            } else {
                self.clear(bit);
            }
        }
        true
    }

    fn valid_range(&self, start: usize, len: usize) -> bool {
        if len == 0 || start >= self.nbits {
            return false;
        }

        start
            .checked_add(len)
            .map(|end| end <= self.nbits)
            .unwrap_or(false)
    }

    fn mask_tail_bits(&mut self) {
        if self.nbits == 0 {
            return;
        }
        let word_bits = usize::BITS as usize;
        let used = self.nbits % word_bits;
        if used == 0 {
            return;
        }
        let mask = (1usize << used) - 1;
        if let Some(last) = self.bits.last_mut() {
            *last &= mask;
        }
    }

    fn word_mask(&self, bit: usize) -> Option<(usize, usize)> {
        if bit >= self.nbits {
            return None;
        }
        let word_bits = usize::BITS as usize;
        let word = bit / word_bits;
        let offset = bit % word_bits;
        Some((word, 1usize << offset))
    }
}
