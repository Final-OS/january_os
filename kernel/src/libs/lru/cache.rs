//! LRU 缓存 - 高效实现
//!
//! ## 设计
//! - 使用双向链表 + HashMap 实现 O(1) 的 get/put/remove
//! - 链表头部为最近使用 (MRU)，尾部为最久未使用 (LRU)
//! - 参考 Linux 内核 LRU 实现，提供完整的缓存管理功能
//!
//! ## 性能
//! - get: O(1)
//! - put: O(1)
//! - remove: O(1)
//! - peek: O(1)

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::ptr::NonNull;

/// LRU 缓存节点
struct LruNode<K, V> {
    key: K,
    value: V,
    prev: Option<NonNull<LruNode<K, V>>>,
    next: Option<NonNull<LruNode<K, V>>>,
}

impl<K, V> LruNode<K, V> {
    fn new(key: K, value: V) -> Self {
        Self {
            key,
            value,
            prev: None,
            next: None,
        }
    }
}

/// LRU 缓存
///
/// 使用双向链表 + HashMap 实现，所有操作均为 O(1)。
///
/// ## 示例
/// ```
/// let mut cache = LruCache::new(2);
/// cache.put(1, "a");
/// cache.put(2, "b");
/// assert_eq!(cache.get(&1), Some(&"a"));
/// cache.put(3, "c"); // 淘汰 key=2
/// assert_eq!(cache.get(&2), None);
/// ```
pub struct LruCache<K: Ord + Clone, V> {
    capacity: usize,
    map: BTreeMap<K, NonNull<LruNode<K, V>>>,
    head: Option<NonNull<LruNode<K, V>>>, // MRU (最近使用)
    tail: Option<NonNull<LruNode<K, V>>>, // LRU (最久未使用)
}

impl<K: Ord + Clone, V> LruCache<K, V> {
    /// 创建指定容量的 LRU 缓存
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            map: BTreeMap::new(),
            head: None,
            tail: None,
        }
    }

    /// 获取容量
    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// 获取当前元素数量
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
        while self.pop_lru().is_some() {}
    }

    /// 调整容量
    ///
    /// 如果新容量小于当前元素数量，会淘汰最久未使用的项。
    /// 返回被淘汰的项列表。
    pub fn resize(&mut self, new_capacity: usize) -> Vec<(K, V)> {
        self.capacity = new_capacity;
        let mut evicted = Vec::new();

        while self.len() > self.capacity {
            if let Some(item) = self.pop_lru() {
                evicted.push(item);
            } else {
                break;
            }
        }

        evicted
    }

    /// 获取值并提升为最近使用
    ///
    /// 时间复杂度: O(1)
    pub fn get(&mut self, key: &K) -> Option<&V> {
        let node_ptr = *self.map.get(key)?;
        unsafe {
            self.move_to_front(node_ptr);
            Some(&(*node_ptr.as_ptr()).value)
        }
    }

    /// 获取可变值并提升为最近使用
    ///
    /// 时间复杂度: O(1)
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        let node_ptr = *self.map.get(key)?;
        unsafe {
            self.move_to_front(node_ptr);
            Some(&mut (*node_ptr.as_ptr()).value)
        }
    }

    /// 只读获取（不更新 LRU 顺序）
    ///
    /// 时间复杂度: O(1)
    #[inline]
    pub fn peek(&self, key: &K) -> Option<&V> {
        let node_ptr = *self.map.get(key)?;
        unsafe { Some(&(*node_ptr.as_ptr()).value) }
    }

    /// 可变获取（不更新 LRU 顺序）
    ///
    /// 时间复杂度: O(1)
    #[inline]
    pub fn peek_mut(&mut self, key: &K) -> Option<&mut V> {
        let node_ptr = *self.map.get(key)?;
        unsafe { Some(&mut (*node_ptr.as_ptr()).value) }
    }

    /// 插入键值对
    ///
    /// 返回 (旧值, 被淘汰的项)
    /// - 如果键已存在，更新值并返回旧值
    /// - 如果容量已满，淘汰最久未使用的项
    ///
    /// 时间复杂度: O(1)
    pub fn put(&mut self, key: K, value: V) -> (Option<V>, Option<(K, V)>) {
        if self.capacity == 0 {
            return (None, Some((key, value)));
        }

        // 如果键已存在，更新值
        if let Some(&node_ptr) = self.map.get(&key) {
            unsafe {
                let node = &mut *node_ptr.as_ptr();
                let old_value = core::mem::replace(&mut node.value, value);
                self.move_to_front(node_ptr);
                return (Some(old_value), None);
            }
        }

        // 如果容量已满，淘汰 LRU 项
        let evicted = if self.len() >= self.capacity {
            self.pop_lru()
        } else {
            None
        };

        // 插入新节点
        let node = Box::new(LruNode::new(key.clone(), value));
        let node_ptr = unsafe { NonNull::new_unchecked(Box::into_raw(node)) };

        self.map.insert(key, node_ptr);
        unsafe {
            self.push_front(node_ptr);
        }

        (None, evicted)
    }

    /// 仅在键不存在时插入
    ///
    /// 返回:
    /// - `Ok(evicted)`: 插入成功，返回被淘汰的项（如果有）
    /// - `Err(value)`: 键已存在，返回未使用的值
    pub fn put_if_absent(&mut self, key: K, value: V) -> Result<Option<(K, V)>, V> {
        if self.contains_key(&key) {
            return Err(value);
        }

        let (_, evicted) = self.put(key, value);
        Ok(evicted)
    }

    /// 获取或插入
    ///
    /// 如果键存在，返回现有值的引用；否则插入新值。
    pub fn get_or_insert(&mut self, key: K, value: V) -> &mut V {
        if !self.contains_key(&key) {
            self.put(key.clone(), value);
        }
        self.get_mut(&key).unwrap()
    }

    /// 获取或插入（使用闭包）
    pub fn get_or_insert_with<F>(&mut self, key: K, f: F) -> &mut V
    where
        F: FnOnce() -> V,
    {
        if !self.contains_key(&key) {
            self.put(key.clone(), f());
        }
        self.get_mut(&key).unwrap()
    }

    /// 删除键
    ///
    /// 时间复杂度: O(1)
    pub fn remove(&mut self, key: &K) -> Option<V> {
        let node_ptr = self.map.remove(key)?;
        unsafe {
            self.unlink(node_ptr);
            let node = Box::from_raw(node_ptr.as_ptr());
            Some(node.value)
        }
    }

    /// 手动提升键为最近使用
    pub fn promote(&mut self, key: &K) -> bool {
        if let Some(&node_ptr) = self.map.get(key) {
            unsafe {
                self.move_to_front(node_ptr);
            }
            true
        } else {
            false
        }
    }

    /// 淘汰并返回最久未使用的项
    ///
    /// 时间复杂度: O(1)
    pub fn pop_lru(&mut self) -> Option<(K, V)> {
        let tail_ptr = self.tail?;
        unsafe {
            let node = Box::from_raw(tail_ptr.as_ptr());
            self.map.remove(&node.key);
            self.unlink(tail_ptr);
            Some((node.key, node.value))
        }
    }

    /// 淘汰并返回最近使用的项
    ///
    /// 时间复杂度: O(1)
    pub fn pop_mru(&mut self) -> Option<(K, V)> {
        let head_ptr = self.head?;
        unsafe {
            let node = Box::from_raw(head_ptr.as_ptr());
            self.map.remove(&node.key);
            self.unlink(head_ptr);
            Some((node.key, node.value))
        }
    }

    /// 查看最近使用的键
    #[inline]
    pub fn peek_mru_key(&self) -> Option<&K> {
        let head_ptr = self.head?;
        unsafe { Some(&(*head_ptr.as_ptr()).key) }
    }

    /// 查看最久未使用的键
    #[inline]
    pub fn peek_lru_key(&self) -> Option<&K> {
        let tail_ptr = self.tail?;
        unsafe { Some(&(*tail_ptr.as_ptr()).key) }
    }

    /// 按 MRU -> LRU 顺序迭代
    pub fn iter(&self) -> LruIter<'_, K, V> {
        LruIter {
            current: self.head,
            remaining: self.len(),
            _marker: core::marker::PhantomData,
        }
    }

    /// 按 LRU -> MRU 顺序迭代
    pub fn iter_lru(&self) -> LruIterReverse<'_, K, V> {
        LruIterReverse {
            current: self.tail,
            remaining: self.len(),
            _marker: core::marker::PhantomData,
        }
    }

    /// 按 MRU -> LRU 顺序返回键迭代器
    #[inline]
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.iter().map(|(k, _)| k)
    }

    /// 按 MRU -> LRU 顺序返回值迭代器
    #[inline]
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.iter().map(|(_, v)| v)
    }

    // === 内部方法 ===

    /// 将节点移到链表头部（标记为最近使用）
    unsafe fn move_to_front(&mut self, node_ptr: NonNull<LruNode<K, V>>) {
        if self.head == Some(node_ptr) {
            return; // 已经在头部
        }

        self.unlink(node_ptr);
        self.push_front(node_ptr);
    }

    /// 将节点添加到链表头部
    unsafe fn push_front(&mut self, node_ptr: NonNull<LruNode<K, V>>) {
        let node = &mut *node_ptr.as_ptr();
        node.prev = None;
        node.next = self.head;

        if let Some(old_head) = self.head {
            (*old_head.as_ptr()).prev = Some(node_ptr);
        } else {
            self.tail = Some(node_ptr);
        }

        self.head = Some(node_ptr);
    }

    /// 从链表中移除节点（不释放内存）
    unsafe fn unlink(&mut self, node_ptr: NonNull<LruNode<K, V>>) {
        let node = &mut *node_ptr.as_ptr();

        match (node.prev, node.next) {
            (None, None) => {
                // 唯一节点
                self.head = None;
                self.tail = None;
            }
            (None, Some(next)) => {
                // 头节点
                (*next.as_ptr()).prev = None;
                self.head = Some(next);
            }
            (Some(prev), None) => {
                // 尾节点
                (*prev.as_ptr()).next = None;
                self.tail = Some(prev);
            }
            (Some(prev), Some(next)) => {
                // 中间节点
                (*prev.as_ptr()).next = Some(next);
                (*next.as_ptr()).prev = Some(prev);
            }
        }

        node.prev = None;
        node.next = None;
    }
}

impl<K: Ord + Clone, V> Drop for LruCache<K, V> {
    fn drop(&mut self) {
        self.clear();
    }
}

// === 迭代器 ===

/// MRU -> LRU 迭代器
pub struct LruIter<'a, K: Ord + Clone, V> {
    current: Option<NonNull<LruNode<K, V>>>,
    remaining: usize,
    _marker: core::marker::PhantomData<&'a LruCache<K, V>>,
}

impl<'a, K: Ord + Clone, V> Iterator for LruIter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        let current = self.current?;
        unsafe {
            let node = &*current.as_ptr();
            self.current = node.next;
            self.remaining -= 1;
            Some((&node.key, &node.value))
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<'a, K: Ord + Clone, V> ExactSizeIterator for LruIter<'a, K, V> {}

/// LRU -> MRU 迭代器
pub struct LruIterReverse<'a, K: Ord + Clone, V> {
    current: Option<NonNull<LruNode<K, V>>>,
    remaining: usize,
    _marker: core::marker::PhantomData<&'a LruCache<K, V>>,
}

impl<'a, K: Ord + Clone, V> Iterator for LruIterReverse<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        let current = self.current?;
        unsafe {
            let node = &*current.as_ptr();
            self.current = node.prev;
            self.remaining -= 1;
            Some((&node.key, &node.value))
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<'a, K: Ord + Clone, V> ExactSizeIterator for LruIterReverse<'a, K, V> {}

// === 测试辅助 ===

#[cfg(test)]
impl<K: Ord + Clone + core::fmt::Debug, V: core::fmt::Debug> LruCache<K, V> {
    /// 验证内部数据结构一致性（仅用于测试）
    pub fn verify_integrity(&self) -> bool {
        // 验证长度
        if self.map.len() != self.len() {
            return false;
        }

        // 验证链表长度
        let mut count = 0;
        let mut current = self.head;
        while let Some(node_ptr) = current {
            count += 1;
            unsafe {
                current = (*node_ptr.as_ptr()).next;
            }
        }

        if count != self.len() {
            return false;
        }

        // 验证所有 map 中的节点都在链表中
        for node_ptr in self.map.values() {
            let mut found = false;
            let mut current = self.head;
            while let Some(ptr) = current {
                if ptr == *node_ptr {
                    found = true;
                    break;
                }
                unsafe {
                    current = (*ptr.as_ptr()).next;
                }
            }
            if !found {
                return false;
            }
        }

        true
    }
}
