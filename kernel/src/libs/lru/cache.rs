//! LRU 缓存

use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;

/// 简单 LRU 缓存
///
/// 说明：
/// - 使用 `VecDeque` 维护访问顺序（队首最旧，队尾最新）
/// - 使用 `BTreeMap` 存储键值
/// - `get`/`put` 为线性更新顺序，容量较小时足够简单可靠
pub struct LruCache<K, V> {
    cap: usize,
    map: BTreeMap<K, V>,
    order: VecDeque<K>,
}

impl<K: Ord + Clone, V> LruCache<K, V> {
    /// 创建指定容量的 LRU
    pub fn new(capacity: usize) -> Self {
        Self {
            cap: capacity,
            map: BTreeMap::new(),
            order: VecDeque::new(),
        }
    }

    /// 容量
    #[inline]
    pub fn capacity(&self) -> usize {
        self.cap
    }

    /// 当前长度
    #[inline]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// 是否为空
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// 是否包含键
    #[inline]
    pub fn contains_key(&self, key: &K) -> bool {
        self.map.contains_key(key)
    }

    /// 清空缓存
    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }

    /// 调整容量（缩容时会淘汰旧项）
    ///
    /// 返回被淘汰的项（按 LRU -> MRU 顺序）。
    pub fn resize(&mut self, new_capacity: usize) -> Vec<(K, V)> {
        self.cap = new_capacity;
        self.evict_overflow()
    }

    /// 获取值并提升为最近使用
    pub fn get(&mut self, key: &K) -> Option<&V> {
        if !self.map.contains_key(key) {
            return None;
        }

        self.touch(key);
        self.map.get(key)
    }

    /// 获取可变值并提升为最近使用
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        if !self.map.contains_key(key) {
            return None;
        }

        self.touch(key);
        self.map.get_mut(key)
    }

    /// 只读获取（不更新 LRU 次序）
    #[inline]
    pub fn peek(&self, key: &K) -> Option<&V> {
        self.map.get(key)
    }

    /// 可变获取（不更新 LRU 次序）
    #[inline]
    pub fn peek_mut(&mut self, key: &K) -> Option<&mut V> {
        self.map.get_mut(key)
    }

    /// 插入键值对
    ///
    /// 返回值：
    /// - `(old, None)`: 仅更新已有键
    /// - `(None, Some((k,v)))`: 新插入并触发淘汰
    /// - `(None, None)`: 新插入，无淘汰
    pub fn put(&mut self, key: K, value: V) -> (Option<V>, Option<(K, V)>) {
        if self.cap == 0 {
            return (None, Some((key, value)));
        }

        let old = self.map.insert(key.clone(), value);
        self.touch(&key);

        if old.is_some() {
            return (old, None);
        }

        let evicted = self.evict_lru_if_needed();

        (None, evicted)
    }

    /// 仅在键不存在时插入
    ///
    /// 返回：
    /// - `Ok(evicted)`: 插入成功，`evicted` 表示是否发生淘汰
    /// - `Err(value)`: 键已存在，返回未被使用的新值
    pub fn put_if_absent(&mut self, key: K, value: V) -> Result<Option<(K, V)>, V> {
        if self.map.contains_key(&key) {
            return Err(value);
        }

        let (_, evicted) = self.put(key, value);
        Ok(evicted)
    }

    /// 删除键
    pub fn remove(&mut self, key: &K) -> Option<V> {
        let val = self.map.remove(key);
        if val.is_some() {
            self.remove_from_order(key);
        }
        val
    }

    /// 手动提升键为最近使用
    pub fn promote(&mut self, key: &K) -> bool {
        if !self.map.contains_key(key) {
            return false;
        }

        self.touch(key);
        true
    }

    /// 淘汰最久未使用项
    pub fn evict_lru(&mut self) -> Option<(K, V)> {
        self.evict_lru_inner()
    }

    /// 淘汰最近使用项
    pub fn evict_recent(&mut self) -> Option<(K, V)> {
        while let Some(key) = self.order.pop_back() {
            if let Some(value) = self.map.remove(&key) {
                return Some((key, value));
            }
        }
        None
    }

    /// 查看最近使用键
    pub fn peek_recent_key(&self) -> Option<&K> {
        self.order.back()
    }

    /// 查看最久未使用键
    pub fn peek_lru_key(&self) -> Option<&K> {
        self.order.front()
    }

    /// 按 LRU -> MRU 顺序迭代键值
    pub fn iter_lru(&self) -> impl Iterator<Item = (&K, &V)> {
        self.order
            .iter()
            .filter_map(|key| self.map.get(key).map(|value| (key, value)))
    }

    /// 按 MRU -> LRU 顺序迭代键值
    pub fn iter_mru(&self) -> impl Iterator<Item = (&K, &V)> {
        self.order
            .iter()
            .rev()
            .filter_map(|key| self.map.get(key).map(|value| (key, value)))
    }

    /// 按 LRU -> MRU 顺序返回键迭代
    #[inline]
    pub fn keys_lru(&self) -> impl Iterator<Item = &K> {
        self.iter_lru().map(|(key, _)| key)
    }

    /// 按 MRU -> LRU 顺序返回键迭代
    #[inline]
    pub fn keys_mru(&self) -> impl Iterator<Item = &K> {
        self.iter_mru().map(|(key, _)| key)
    }

    /// 按 LRU -> MRU 顺序返回值迭代
    #[inline]
    pub fn values_lru(&self) -> impl Iterator<Item = &V> {
        self.iter_lru().map(|(_, value)| value)
    }

    /// 按 MRU -> LRU 顺序返回值迭代
    #[inline]
    pub fn values_mru(&self) -> impl Iterator<Item = &V> {
        self.iter_mru().map(|(_, value)| value)
    }

    fn touch(&mut self, key: &K) {
        self.remove_from_order(key);
        self.order.push_back(key.clone());
    }

    fn remove_from_order(&mut self, key: &K) {
        if let Some(pos) = self.order.iter().position(|k| k == key) {
            self.order.remove(pos);
        }
    }

    fn evict_lru_if_needed(&mut self) -> Option<(K, V)> {
        if self.map.len() <= self.cap {
            return None;
        }

        self.evict_lru_inner()
    }

    fn evict_lru_inner(&mut self) -> Option<(K, V)> {
        while let Some(key) = self.order.pop_front() {
            if let Some(value) = self.map.remove(&key) {
                return Some((key, value));
            }
        }

        None
    }

    fn evict_overflow(&mut self) -> Vec<(K, V)> {
        let mut evicted = Vec::new();

        while self.map.len() > self.cap {
            match self.evict_lru_inner() {
                Some(item) => evicted.push(item),
                None => break,
            }
        }

        evicted
    }
}
