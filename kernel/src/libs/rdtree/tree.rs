//! Radix Tree（简化）
//!
//! 这里提供内核常用的“整数索引 -> 对象”接口。
//! 当前实现使用 `BTreeMap` 承载，后续可无缝替换为真正多级基数树。

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::ops::{Bound, RangeBounds};

/// 简化基数树
pub struct RadixTree<V> {
    inner: BTreeMap<usize, V>,
}

impl<V> RadixTree<V> {
    /// 创建空树
    pub const fn new() -> Self {
        Self {
            inner: BTreeMap::new(),
        }
    }

    /// 数量
    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// 是否为空
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// 是否包含索引
    #[inline]
    pub fn contains(&self, index: usize) -> bool {
        self.inner.contains_key(&index)
    }

    /// 插入索引
    #[inline]
    pub fn insert(&mut self, index: usize, value: V) -> Option<V> {
        self.inner.insert(index, value)
    }

    /// 获取
    #[inline]
    pub fn get(&self, index: usize) -> Option<&V> {
        self.inner.get(&index)
    }

    /// 获取可变引用
    #[inline]
    pub fn get_mut(&mut self, index: usize) -> Option<&mut V> {
        self.inner.get_mut(&index)
    }

    /// 获取键值对
    #[inline]
    pub fn get_key_value(&self, index: usize) -> Option<(usize, &V)> {
        self.inner.get_key_value(&index).map(|(k, v)| (*k, v))
    }

    /// 删除
    #[inline]
    pub fn remove(&mut self, index: usize) -> Option<V> {
        self.inner.remove(&index)
    }

    /// 删除并返回键值对
    #[inline]
    pub fn remove_entry(&mut self, index: usize) -> Option<(usize, V)> {
        self.inner.remove_entry(&index)
    }

    /// 最小项
    #[inline]
    pub fn first(&self) -> Option<(usize, &V)> {
        self.inner.first_key_value().map(|(k, v)| (*k, v))
    }

    /// 最大项
    #[inline]
    pub fn last(&self) -> Option<(usize, &V)> {
        self.inner.last_key_value().map(|(k, v)| (*k, v))
    }

    /// 弹出最小项
    #[inline]
    pub fn pop_first(&mut self) -> Option<(usize, V)> {
        self.inner.pop_first()
    }

    /// 弹出最大项
    #[inline]
    pub fn pop_last(&mut self) -> Option<(usize, V)> {
        self.inner.pop_last()
    }

    /// 返回第一个 `>= index` 的项
    #[inline]
    pub fn lower_bound(&self, index: usize) -> Option<(usize, &V)> {
        self.inner
            .range((Bound::Included(index), Bound::Unbounded))
            .next()
            .map(|(k, v)| (*k, v))
    }

    /// 返回第一个 `> index` 的项
    #[inline]
    pub fn upper_bound(&self, index: usize) -> Option<(usize, &V)> {
        self.inner
            .range((Bound::Excluded(index), Bound::Unbounded))
            .next()
            .map(|(k, v)| (*k, v))
    }

    /// 返回最后一个 `<= index` 的项
    #[inline]
    pub fn floor(&self, index: usize) -> Option<(usize, &V)> {
        self.inner
            .range((Bound::Unbounded, Bound::Included(index)))
            .next_back()
            .map(|(k, v)| (*k, v))
    }

    /// 返回最后一个 `< index` 的项
    #[inline]
    pub fn lower_than(&self, index: usize) -> Option<(usize, &V)> {
        self.inner
            .range((Bound::Unbounded, Bound::Excluded(index)))
            .next_back()
            .map(|(k, v)| (*k, v))
    }

    /// 返回 [start, end] 范围内第一个项
    pub fn first_in_range(&self, start: usize, end: usize) -> Option<(usize, &V)> {
        self.inner.range(start..=end).next().map(|(k, v)| (*k, v))
    }

    /// 返回 [start, end] 范围内最后一个项
    pub fn last_in_range(&self, start: usize, end: usize) -> Option<(usize, &V)> {
        self.inner
            .range(start..=end)
            .next_back()
            .map(|(k, v)| (*k, v))
    }

    /// 范围迭代（按索引有序）
    #[inline]
    pub fn range<R>(&self, range: R) -> impl Iterator<Item = (usize, &V)>
    where
        R: RangeBounds<usize>,
    {
        self.inner.range(range).map(|(k, v)| (*k, v))
    }

    /// 可变范围迭代（按索引有序）
    #[inline]
    pub fn range_mut<R>(&mut self, range: R) -> impl Iterator<Item = (usize, &mut V)>
    where
        R: RangeBounds<usize>,
    {
        self.inner.range_mut(range).map(|(k, v)| (*k, v))
    }

    /// 清空
    #[inline]
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// 删除范围 `[start, end]` 内的所有项
    ///
    /// 返回删除的键值对（按索引递增顺序）。
    pub fn remove_range(&mut self, start: usize, end: usize) -> Vec<(usize, V)> {
        if start > end {
            return Vec::new();
        }

        let keys: Vec<usize> = self.inner.range(start..=end).map(|(k, _)| *k).collect();
        let mut removed = Vec::with_capacity(keys.len());

        for key in keys {
            if let Some(value) = self.inner.remove(&key) {
                removed.push((key, value));
            }
        }

        removed
    }

    /// 合并另一棵树（同键覆盖）
    #[inline]
    pub fn append(&mut self, other: &mut Self) {
        self.inner.append(&mut other.inner);
    }

    /// 保留满足条件的元素
    #[inline]
    pub fn retain<F>(&mut self, f: F)
    where
        F: FnMut(&usize, &mut V) -> bool,
    {
        self.inner.retain(f);
    }

    /// 查找从 `start` 开始第一个空闲索引（用于 ID 分配）
    pub fn find_first_gap_from(&self, start: usize) -> Option<usize> {
        let mut expect = start;

        for (&index, _) in self.inner.range(start..) {
            if index != expect {
                return Some(expect);
            }

            if expect == usize::MAX {
                return None;
            }
            expect += 1;
        }

        Some(expect)
    }

    /// 在第一个空闲索引处插入
    ///
    /// 返回分配到的索引。若不存在可用索引（索引空间耗尽）返回 `None`。
    pub fn insert_first_gap_from(&mut self, start: usize, value: V) -> Option<usize> {
        let index = self.find_first_gap_from(start)?;
        let _old = self.inner.insert(index, value);
        debug_assert!(_old.is_none());
        Some(index)
    }

    /// 有序迭代
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = (usize, &V)> {
        self.inner.iter().map(|(k, v)| (*k, v))
    }

    /// 有序可变迭代
    #[inline]
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (usize, &mut V)> {
        self.inner.iter_mut().map(|(k, v)| (*k, v))
    }
}

impl<V> Default for RadixTree<V> {
    fn default() -> Self {
        Self::new()
    }
}
