//! Maple Tree — 基于 B-Tree 的区间树实现
//!
//! 使用通用 B-Tree 作为底层存储，添加区间特定的功能。

use alloc::vec::Vec;
use core::fmt;

use crate::libs::btree::BTree;

/// 区间插入错误
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapleInsertError {
    InvalidRange,
    Overlap,
}

/// 区间条目（内部使用）
#[derive(Clone)]
struct RangeEntry<V> {
    start: usize,
    end: usize,
    value: V,
}

/// Maple Tree - 区间树
///
/// 使用 `[start, end)` 半开区间，不允许重叠。
pub struct MapleTree<V> {
    // 使用 start 作为键，RangeEntry 作为值
    tree: BTree<usize, RangeEntry<V>>,
}

impl<V> MapleTree<V> {
    /// 创建空树
    pub fn new() -> Self {
        Self {
            tree: BTree::new(),
        }
    }

    /// 元素数量
    pub fn len(&self) -> usize {
        self.tree.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.tree.is_empty()
    }

    /// 清空树
    pub fn clear(&mut self) {
        self.tree = BTree::new();
    }

    /// 插入区间
    pub fn insert(&mut self, start: usize, end: usize, value: V) -> Result<Option<V>, MapleInsertError> {
        // 验证区间
        if start >= end {
            return Err(MapleInsertError::InvalidRange);
        }

        // 检查重叠
        if self.has_overlap(start, end) {
            return Err(MapleInsertError::Overlap);
        }

        // 插入
        let entry = RangeEntry { start, end, value };
        let old = self.tree.insert(start, entry);

        Ok(old.map(|e| e.value))
    }

    /// 检查是否有重叠
    fn has_overlap(&self, start: usize, end: usize) -> bool {
        for (_, entry) in self.tree.iter() {
            if !(end <= entry.start || start >= entry.end) {
                return true;
            }
        }
        false
    }

    /// 查找包含指定点的区间
    pub fn find(&self, point: usize) -> Option<(usize, usize, &V)> {
        for (_, entry) in self.tree.iter() {
            if point >= entry.start && point < entry.end {
                return Some((entry.start, entry.end, &entry.value));
            }
        }
        None
    }

    /// 删除区间
    pub fn remove(&mut self, start: usize) -> Option<(usize, V)> {
        self.tree.remove(&start).map(|entry| (entry.end, entry.value))
    }

    /// 查找间隙（正向）
    pub fn find_gap(&self, size: usize, start: usize, end: usize) -> Option<usize> {
        if start >= end || size == 0 {
            return None;
        }

        let mut ranges: Vec<_> = self.tree.iter()
            .map(|(_, e)| (e.start, e.end))
            .collect();
        ranges.sort_by_key(|(s, _)| *s);

        // 检查第一个区间前的间隙
        if ranges.is_empty() || ranges[0].0 >= start + size {
            let gap_start = start;
            let gap_end = ranges.first().map(|(s, _)| *s).unwrap_or(end);
            if gap_end - gap_start >= size {
                return Some(gap_start);
            }
        }

        // 检查区间之间的间隙
        for i in 0..ranges.len().saturating_sub(1) {
            let gap_start = ranges[i].1.max(start);
            let gap_end = ranges[i + 1].0.min(end);

            if gap_end > gap_start && gap_end - gap_start >= size {
                return Some(gap_start);
            }
        }

        // 检查最后一个区间后的间隙
        if let Some((_, last_end)) = ranges.last() {
            let gap_start = (*last_end).max(start);
            if gap_start < end && end - gap_start >= size {
                return Some(gap_start);
            }
        }

        None
    }

    /// 查找间隙（反向）
    pub fn find_gap_reverse(&self, size: usize, start: usize, end: usize) -> Option<usize> {
        if start <= end || size == 0 {
            return None;
        }

        let mut ranges: Vec<_> = self.tree.iter()
            .map(|(_, e)| (e.start, e.end))
            .collect();
        ranges.sort_by_key(|(s, _)| *s);

        // 检查最后一个区间后的间隙 (从 start 向下搜索到 end)
        if let Some((_, last_end)) = ranges.last() {
            let gap_start = (*last_end).max(end);
            let gap_end = start;
            if gap_end > gap_start && gap_end - gap_start >= size {
                return Some(gap_end - size);
            }
        }

        // 检查区间之间的间隙（从后向前）
        for i in (0..ranges.len().saturating_sub(1)).rev() {
            let gap_start = ranges[i].1.max(end);
            let gap_end = ranges[i + 1].0.min(start);

            if gap_end > gap_start && gap_end - gap_start >= size {
                return Some(gap_end - size);
            }
        }

        // 检查第一个区间前的间隙
        if ranges.is_empty() {
            if start - end >= size {
                return Some(start - size);
            }
        } else if ranges[0].0 > end {
            let gap_start = end;
            let gap_end = ranges[0].0.min(start);
            if gap_end > gap_start && gap_end - gap_start >= size {
                return Some(gap_end - size);
            }
        }

        None
    }

    /// 迭代所有区间
    pub fn iter(&self) -> impl Iterator<Item = (usize, usize, &V)> {
        self.tree.iter().map(|(_, entry)| (entry.start, entry.end, &entry.value))
    }

    /// 更新区间的结束位置
    pub fn update_end(&mut self, start: usize, new_end: usize) -> Result<(), MapleInsertError> {
        if let Some((old_end, value)) = self.remove(start) {
            if self.has_overlap(start, new_end) {
                let _ = self.insert(start, old_end, value);
                return Err(MapleInsertError::Overlap);
            }
            self.insert(start, new_end, value).map(|_| ())
        } else {
            Err(MapleInsertError::InvalidRange)
        }
    }

    /// 替换区间
    pub fn replace(
        &mut self,
        start: usize,
        end: usize,
        value: V,
    ) -> Result<Option<(usize, V)>, MapleInsertError> {
        let old = self.remove(start);
        match self.insert(start, end, value) {
            Ok(_) => Ok(old),
            Err(e) => {
                if let Some((old_end, old_value)) = old {
                    let _ = self.insert(start, old_end, old_value);
                }
                Err(e)
            }
        }
    }

    /// 插入并覆盖重叠的区间
    pub fn insert_overwrite(
        &mut self,
        start: usize,
        end: usize,
        value: V,
    ) -> Result<Vec<(usize, usize, V)>, MapleInsertError> {
        if start >= end {
            return Err(MapleInsertError::InvalidRange);
        }

        let mut removed = Vec::new();
        let overlapping: Vec<usize> = self.tree.iter()
            .filter(|(_, e)| !(end <= e.start || start >= e.end))
            .map(|(s, _)| *s)
            .collect();

        for s in overlapping {
            if let Some((e, v)) = self.remove(s) {
                removed.push((s, e, v));
            }
        }

        self.insert(start, end, value)?;
        Ok(removed)
    }

    /// 在指定位置分割区间
    pub fn split_at(&mut self, point: usize) -> bool
    where
        V: Clone,
    {
        let entry = self.tree.iter()
            .find(|(_, e)| point > e.start && point < e.end)
            .map(|(s, e)| (*s, e.end, e.value.clone()));

        if let Some((start, end, value)) = entry {
            if let Some((_, _)) = self.remove(start) {
                let _ = self.insert(start, point, value.clone());
                let _ = self.insert(point, end, value);
                return true;
            }
        }

        false
    }

    /// 合并相邻且值相等的区间
    pub fn merge_adjacent_equal(&mut self) -> usize
    where
        V: PartialEq + Clone,
    {
        let mut merged_count = 0;

        loop {
            let mut ranges: Vec<_> = self.tree.iter()
                .map(|(_, e)| (e.start, e.end, e.value.clone()))
                .collect();
            ranges.sort_by_key(|(s, _, _)| *s);

            let to_merge = ranges.windows(2)
                .find(|w| w[0].1 == w[1].0 && w[0].2 == w[1].2)
                .map(|w| (w[0].0, w[0].1, w[1].0, w[1].1, w[0].2.clone()));

            if let Some((s1, e1, s2, e2, v)) = to_merge {
                if let Some((_, _)) = self.remove(s1) {
                    let _ = self.remove(s2);
                    let _ = self.insert(s1, e2, v);
                    merged_count += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        merged_count
    }

    /// 查找第一个 >= start 的区间
    pub fn lower_bound(&self, start: usize) -> Option<(usize, usize, &V)> {
        self.tree.iter()
            .find(|(_, e)| e.start >= start)
            .map(|(_, e)| (e.start, e.end, &e.value))
    }

    /// 迭代与指定范围相交的区间
    pub fn iter_intersecting(&self, start: usize, end: usize) -> impl Iterator<Item = (usize, usize, &V)> {
        self.tree.iter()
            .filter(move |(_, e)| !(end <= e.start || start >= e.end))
            .map(|(_, e)| (e.start, e.end, &e.value))
    }

    /// 获取指定点的可变引用
    pub fn get_mut(&mut self, point: usize) -> Option<&mut V> {
        // 找到包含该点的区间
        for (_, entry) in self.tree.iter_mut() {
            if point >= entry.start && point < entry.end {
                return Some(&mut entry.value);
            }
        }
        None
    }

    /// 获取指定起始位置的区间的可变引用
    pub fn get_mut_at(&mut self, start: usize) -> Option<(usize, &mut V)> {
        self.tree.get_mut(&start).map(|e| (e.end, &mut e.value))
    }

    /// 获取第一个区间
    pub fn first(&self) -> Option<(usize, usize, &V)> {
        self.tree.first().map(|(_, e)| (e.start, e.end, &e.value))
    }

    /// 获取最后一个区间
    pub fn last(&self) -> Option<(usize, usize, &V)> {
        self.tree.last().map(|(_, e)| (e.start, e.end, &e.value))
    }

    /// 弹出第一个区间
    pub fn pop_first(&mut self) -> Option<(usize, usize, V)> {
        self.tree.pop_first().map(|(_, e)| (e.start, e.end, e.value))
    }

    /// 弹出最后一个区间
    pub fn pop_last(&mut self) -> Option<(usize, usize, V)> {
        self.tree.pop_last().map(|(_, e)| (e.start, e.end, e.value))
    }

    /// 获取所有起始位置
    pub fn starts(&self) -> impl Iterator<Item = usize> + '_ {
        self.tree.keys().copied()
    }

    /// 获取所有值的引用
    pub fn values(&self) -> impl Iterator<Item = &V> + '_ {
        self.tree.values().map(|e| &e.value)
    }

    /// 获取所有值的可变引用
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut V> + '_ {
        self.tree.values_mut().map(|e| &mut e.value)
    }

    /// 检查是否包含指定点
    pub fn contains(&self, point: usize) -> bool {
        self.find(point).is_some()
    }

    /// 检查是否包含指定起始位置的区间
    pub fn contains_start(&self, start: usize) -> bool {
        self.tree.contains_key(&start)
    }

    /// 保留满足条件的区间
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(usize, usize, &V) -> bool,
    {
        self.tree.retain(|_, e| f(e.start, e.end, &e.value));
    }

    /// 移除所有与指定范围相交的区间
    pub fn remove_intersecting(&mut self, start: usize, end: usize) -> Vec<(usize, usize, V)> {
        let to_remove: Vec<_> = self.tree.iter()
            .filter(|(_, e)| !(end <= e.start || start >= e.end))
            .map(|(k, _)| *k)
            .collect();

        to_remove.into_iter()
            .filter_map(|k| self.tree.remove(&k))
            .map(|e| (e.start, e.end, e.value))
            .collect()
    }

    /// 获取区间总长度
    pub fn total_length(&self) -> usize {
        self.tree.values().map(|e| e.end - e.start).sum()
    }

    /// 检查是否有任何区间与指定范围相交
    pub fn has_intersection(&self, start: usize, end: usize) -> bool {
        self.tree.iter().any(|(_, e)| !(end <= e.start || start >= e.end))
    }
}


impl<V> Default for MapleTree<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: Clone> Clone for MapleTree<V> {
    fn clone(&self) -> Self {
        let mut new_tree = Self::new();
        for (start, end, value) in self.iter() {
            let _ = new_tree.insert(start, end, value.clone());
        }
        new_tree
    }
}

impl<V: fmt::Debug> fmt::Debug for MapleTree<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map()
            .entries(self.iter().map(|(s, e, v)| ((s, e), v)))
            .finish()
    }
}
