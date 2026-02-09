//! Radix Tree（多级基数树实现）
//!
//! 这是一个多级基数树实现，用于"整数索引 -> 对象"的映射。
//!
//! ## 设计
//! - 每个节点有 64 个槽位（6-bit 索引）
//! - 对于 64-bit usize，最多需要 11 层
//! - 稀疏存储：只分配实际使用的节点
//! - 时间复杂度：O(log_64(N)) ≈ O(1) 对于实际的键范围

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ops::RangeBounds;

/// 每个节点的槽位数（2^SHIFT）
const RADIX: usize = 64;

/// 每级索引的位数
const SHIFT: usize = 6;

/// 掩码，用于提取每级的索引
const MASK: usize = RADIX - 1;

/// 计算给定键需要的树高度
#[inline]
const fn height_for_key(key: usize) -> usize {
    if key == 0 {
        return 1;
    }
    let bits = usize::BITS as usize - key.leading_zeros() as usize;
    (bits + SHIFT - 1) / SHIFT
}

/// 提取键在指定层级的索引
#[inline]
const fn key_index(key: usize, level: usize) -> usize {
    (key >> (level * SHIFT)) & MASK
}

/// 节点或值的枚举
enum Slot<V> {
    /// 内部节点
    Node(Box<Node<V>>),
    /// 叶子值
    Value(V),
}

/// 基数树节点
struct Node<V> {
    /// 槽位数组
    slots: [Option<Box<Slot<V>>>; RADIX],
    /// 此节点子树中的元素数量
    count: usize,
}

impl<V> Node<V> {
    /// 创建空节点
    fn new() -> Self {
        Self {
            slots: core::array::from_fn(|_| None),
            count: 0,
        }
    }

    /// 获取槽位
    #[inline]
    fn get_slot(&self, index: usize) -> Option<&Slot<V>> {
        self.slots[index].as_deref()
    }

    /// 获取可变槽位
    #[inline]
    fn get_slot_mut(&mut self, index: usize) -> Option<&mut Slot<V>> {
        self.slots[index].as_deref_mut()
    }

    /// 设置槽位
    #[inline]
    fn set_slot(&mut self, index: usize, slot: Slot<V>) {
        self.slots[index] = Some(Box::new(slot));
    }

    /// 移除槽位
    #[inline]
    fn remove_slot(&mut self, index: usize) -> Option<Box<Slot<V>>> {
        self.slots[index].take()
    }

    /// 是否为空
    #[inline]
    fn is_empty(&self) -> bool {
        self.count == 0
    }
}

/// 简化基数树
pub struct RadixTree<V> {
    root: Option<Box<Node<V>>>,
    len: usize,
    height: usize,
}

impl<V> RadixTree<V> {
    /// 创建空树
    pub const fn new() -> Self {
        Self {
            root: None,
            len: 0,
            height: 0,
        }
    }

    /// 数量
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// 是否为空
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// 确保树有足够的高度
    fn ensure_height(&mut self, required_height: usize) {
        while self.height < required_height {
            let mut new_root = Node::new();
            if let Some(old_root) = self.root.take() {
                new_root.slots[0] = Some(Box::new(Slot::Node(old_root)));
                new_root.count = self.len;
            }
            self.root = Some(Box::new(new_root));
            self.height += 1;
        }
    }

    /// 插入索引
    pub fn insert(&mut self, index: usize, value: V) -> Option<V> {
        let required_height = height_for_key(index);
        self.ensure_height(required_height);

        let root = self.root.as_mut().unwrap();
        let old_value = Self::insert_at(root, index, value, self.height - 1);

        if old_value.is_none() {
            self.len += 1;
        }

        old_value
    }

    /// 在指定节点插入
    fn insert_at(node: &mut Node<V>, key: usize, value: V, level: usize) -> Option<V> {
        let idx = key_index(key, level);

        if level == 0 {
            // 叶子层：直接插入值
            let old = node.remove_slot(idx);
            node.set_slot(idx, Slot::Value(value));

            if old.is_none() {
                node.count += 1;
            }

            match old {
                Some(slot) => match *slot {
                    Slot::Value(v) => Some(v),
                    _ => unreachable!(),
                },
                None => None,
            }
        } else {
            // 内部层：递归插入
            let child = match node.get_slot_mut(idx) {
                Some(Slot::Node(child)) => child,
                _ => {
                    // 创建新子节点
                    let new_child = Box::new(Node::new());
                    node.set_slot(idx, Slot::Node(new_child));
                    match node.get_slot_mut(idx).unwrap() {
                        Slot::Node(child) => child,
                        _ => unreachable!(),
                    }
                }
            };

            let old = Self::insert_at(child, key, value, level - 1);
            if old.is_none() {
                node.count += 1;
            }
            old
        }
    }

    /// 是否包含索引
    #[inline]
    pub fn contains(&self, index: usize) -> bool {
        self.get(index).is_some()
    }

    /// 获取
    pub fn get(&self, index: usize) -> Option<&V> {
        let required_height = height_for_key(index);
        if required_height > self.height {
            return None;
        }

        let root = self.root.as_ref()?;
        Self::get_at(root, index, self.height - 1)
    }

    /// 在指定节点获取
    fn get_at(node: &Node<V>, key: usize, level: usize) -> Option<&V> {
        let idx = key_index(key, level);

        match node.get_slot(idx)? {
            Slot::Value(v) if level == 0 => Some(v),
            Slot::Node(child) if level > 0 => Self::get_at(child, key, level - 1),
            _ => None,
        }
    }

    /// 获取可变引用
    pub fn get_mut(&mut self, index: usize) -> Option<&mut V> {
        let required_height = height_for_key(index);
        if required_height > self.height {
            return None;
        }

        let root = self.root.as_mut()?;
        Self::get_mut_at(root, index, self.height - 1)
    }

    /// 在指定节点获取可变引用
    fn get_mut_at(node: &mut Node<V>, key: usize, level: usize) -> Option<&mut V> {
        let idx = key_index(key, level);

        match node.get_slot_mut(idx)? {
            Slot::Value(v) if level == 0 => Some(v),
            Slot::Node(child) if level > 0 => Self::get_mut_at(child, key, level - 1),
            _ => None,
        }
    }

    /// 获取键值对
    #[inline]
    pub fn get_key_value(&self, index: usize) -> Option<(usize, &V)> {
        self.get(index).map(|v| (index, v))
    }

    /// 删除
    pub fn remove(&mut self, index: usize) -> Option<V> {
        let required_height = height_for_key(index);
        if required_height > self.height {
            return None;
        }

        let root = self.root.as_mut()?;
        let value = Self::remove_at(root, index, self.height - 1)?;

        self.len -= 1;

        // 如果根节点为空，减少树高度
        while self.height > 0 && self.root.as_ref().unwrap().is_empty() {
            self.root = None;
            self.height = 0;
        }

        Some(value)
    }

    /// 在指定节点删除
    fn remove_at(node: &mut Node<V>, key: usize, level: usize) -> Option<V> {
        let idx = key_index(key, level);

        if level == 0 {
            // 叶子层：直接删除
            let slot = node.remove_slot(idx)?;
            node.count -= 1;

            match *slot {
                Slot::Value(v) => Some(v),
                _ => unreachable!(),
            }
        } else {
            // 内部层：递归删除
            // 先递归删除并检查是否为空
            let (value, is_empty) = {
                let child = match node.get_slot_mut(idx)? {
                    Slot::Node(child) => child,
                    _ => return None,
                };

                let value = Self::remove_at(child, key, level - 1)?;
                let empty = child.is_empty();
                (value, empty)
            };

            // 现在可以安全地修改 node
            node.count -= 1;

            // 如果子节点为空，删除它
            if is_empty {
                node.remove_slot(idx);
            }

            Some(value)
        }
    }

    /// 删除并返回键值对
    #[inline]
    pub fn remove_entry(&mut self, index: usize) -> Option<(usize, V)> {
        self.remove(index).map(|v| (index, v))
    }

    /// 清空
    pub fn clear(&mut self) {
        self.root = None;
        self.len = 0;
        self.height = 0;
    }

    /// 最小项
    pub fn first(&self) -> Option<(usize, &V)> {
        let root = self.root.as_ref()?;
        Self::find_first(root, 0, self.height - 1)
    }

    /// 查找最小项
    fn find_first(node: &Node<V>, prefix: usize, level: usize) -> Option<(usize, &V)> {
        for i in 0..RADIX {
            if let Some(slot) = node.get_slot(i) {
                let key = prefix | (i << (level * SHIFT));
                match slot {
                    Slot::Value(v) if level == 0 => return Some((key, v)),
                    Slot::Node(child) if level > 0 => {
                        if let Some(result) = Self::find_first(child, key, level - 1) {
                            return Some(result);
                        }
                    }
                    _ => {}
                }
            }
        }
        None
    }

    /// 最大项
    pub fn last(&self) -> Option<(usize, &V)> {
        let root = self.root.as_ref()?;
        Self::find_last(root, 0, self.height - 1)
    }

    /// 查找最大项
    fn find_last(node: &Node<V>, prefix: usize, level: usize) -> Option<(usize, &V)> {
        for i in (0..RADIX).rev() {
            if let Some(slot) = node.get_slot(i) {
                let key = prefix | (i << (level * SHIFT));
                match slot {
                    Slot::Value(v) if level == 0 => return Some((key, v)),
                    Slot::Node(child) if level > 0 => {
                        if let Some(result) = Self::find_last(child, key, level - 1) {
                            return Some(result);
                        }
                    }
                    _ => {}
                }
            }
        }
        None
    }

    /// 弹出最小项
    pub fn pop_first(&mut self) -> Option<(usize, V)> {
        let (key, _) = self.first()?;
        self.remove(key).map(|v| (key, v))
    }

    /// 弹出最大项
    pub fn pop_last(&mut self) -> Option<(usize, V)> {
        let (key, _) = self.last()?;
        self.remove(key).map(|v| (key, v))
    }

    /// 返回第一个 `>= index` 的项
    pub fn lower_bound(&self, index: usize) -> Option<(usize, &V)> {
        let root = self.root.as_ref()?;
        Self::find_lower_bound(root, 0, self.height - 1, index)
    }

    /// 查找 lower_bound
    fn find_lower_bound(
        node: &Node<V>,
        prefix: usize,
        level: usize,
        target: usize,
    ) -> Option<(usize, &V)> {
        let target_idx = key_index(target, level);

        // 先尝试目标索引及之后的槽位
        for i in target_idx..RADIX {
            if let Some(slot) = node.get_slot(i) {
                let key = prefix | (i << (level * SHIFT));

                if level == 0 {
                    if let Slot::Value(v) = slot {
                        if key >= target {
                            return Some((key, v));
                        }
                    }
                } else if let Slot::Node(child) = slot {
                    // 如果是目标索引，递归查找
                    if i == target_idx {
                        if let Some(result) = Self::find_lower_bound(child, key, level - 1, target)
                        {
                            return Some(result);
                        }
                    } else {
                        // 如果是之后的索引，找第一个
                        if let Some(result) = Self::find_first(child, key, level - 1) {
                            return Some(result);
                        }
                    }
                }
            }
        }

        None
    }

    /// 返回第一个 `> index` 的项
    pub fn upper_bound(&self, index: usize) -> Option<(usize, &V)> {
        // upper_bound(x) = lower_bound(x + 1)
        let next = index.checked_add(1)?;
        self.lower_bound(next)
    }

    /// 返回最后一个 `<= index` 的项
    pub fn floor(&self, index: usize) -> Option<(usize, &V)> {
        let root = self.root.as_ref()?;
        Self::find_floor(root, 0, self.height - 1, index)
    }

    /// 查找 floor
    fn find_floor(
        node: &Node<V>,
        prefix: usize,
        level: usize,
        target: usize,
    ) -> Option<(usize, &V)> {
        let target_idx = key_index(target, level);

        // 从目标索引向前查找
        for i in (0..=target_idx).rev() {
            if let Some(slot) = node.get_slot(i) {
                let key = prefix | (i << (level * SHIFT));

                if level == 0 {
                    if let Slot::Value(v) = slot {
                        if key <= target {
                            return Some((key, v));
                        }
                    }
                } else if let Slot::Node(child) = slot {
                    // 如果是目标索引，递归查找
                    if i == target_idx {
                        if let Some(result) = Self::find_floor(child, key, level - 1, target) {
                            return Some(result);
                        }
                    } else {
                        // 如果是之前的索引，找最后一个
                        if let Some(result) = Self::find_last(child, key, level - 1) {
                            return Some(result);
                        }
                    }
                }
            }
        }

        None
    }

    /// 返回最后一个 `< index` 的项
    pub fn lower_than(&self, index: usize) -> Option<(usize, &V)> {
        if index == 0 {
            return None;
        }
        self.floor(index - 1)
    }

    /// 返回 [start, end] 范围内第一个项
    pub fn first_in_range(&self, start: usize, end: usize) -> Option<(usize, &V)> {
        if start > end {
            return None;
        }
        let (key, val) = self.lower_bound(start)?;
        if key <= end {
            Some((key, val))
        } else {
            None
        }
    }

    /// 返回 [start, end] 范围内最后一个项
    pub fn last_in_range(&self, start: usize, end: usize) -> Option<(usize, &V)> {
        if start > end {
            return None;
        }
        let (key, val) = self.floor(end)?;
        if key >= start {
            Some((key, val))
        } else {
            None
        }
    }

    /// 有序迭代
    pub fn iter(&self) -> Iter<'_, V> {
        Iter {
            stack: Vec::new(),
            tree: self,
        }
    }

    /// 有序可变迭代
    pub fn iter_mut(&mut self) -> IterMut<'_, V> {
        IterMut {
            stack: Vec::new(),
            tree: self as *mut _,
            _marker: core::marker::PhantomData,
        }
    }

    /// 获取所有键的迭代器
    pub fn keys(&self) -> impl Iterator<Item = usize> + '_ {
        self.iter().map(|(k, _)| k)
    }

    /// 获取所有值的迭代器
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.iter().map(|(_, v)| v)
    }

    /// 获取所有值的可变迭代器
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.iter_mut().map(|(_, v)| v)
    }

    /// 查找从 `start` 开始第一个空闲索引（用于 ID 分配）
    pub fn find_first_gap_from(&self, start: usize) -> Option<usize> {
        let mut current = start;

        // 检查从 start 开始的每个位置，直到找到一个空闲的
        loop {
            if !self.contains(current) {
                return Some(current);
            }
            current = current.checked_add(1)?;
        }
    }

    /// 在第一个空闲索引处插入
    pub fn insert_first_gap_from(&mut self, start: usize, value: V) -> Option<usize> {
        let index = self.find_first_gap_from(start)?;
        let _old = self.insert(index, value);
        debug_assert!(_old.is_none());
        Some(index)
    }

    /// 删除范围 `[start, end]` 内的所有项
    pub fn remove_range(&mut self, start: usize, end: usize) -> Vec<(usize, V)> {
        if start > end {
            return Vec::new();
        }

        // 收集需要删除的键
        let keys: Vec<usize> = self
            .iter()
            .filter_map(|(k, _)| if k >= start && k <= end { Some(k) } else { None })
            .collect();

        let mut removed = Vec::with_capacity(keys.len());
        for key in keys {
            if let Some(value) = self.remove(key) {
                removed.push((key, value));
            }
        }

        removed
    }

    /// 合并另一棵树（同键覆盖）
    pub fn append(&mut self, other: &mut Self) {
        // 收集 other 中的所有项
        let items: Vec<(usize, V)> = core::mem::take(other).into_iter().collect();

        // 插入到 self 中
        for (key, value) in items {
            self.insert(key, value);
        }
    }

    /// 保留满足条件的元素
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&usize, &mut V) -> bool,
    {
        let keys_to_remove: Vec<usize> = self
            .iter_mut()
            .filter_map(|(k, v)| if !f(&k, v) { Some(k) } else { None })
            .collect();

        for key in keys_to_remove {
            self.remove(key);
        }
    }

    /// 在指定索引处分割树
    pub fn split_off(&mut self, index: usize) -> Self {
        let mut new_tree = Self::new();

        // 收集 >= index 的键
        let keys: Vec<usize> = self
            .iter()
            .filter_map(|(k, _)| if k >= index { Some(k) } else { None })
            .collect();

        // 移动到新树
        for key in keys {
            if let Some(value) = self.remove(key) {
                new_tree.insert(key, value);
            }
        }

        new_tree
    }

    /// Entry API：获取或插入
    pub fn get_or_insert(&mut self, index: usize, default: V) -> &mut V {
        if !self.contains(index) {
            self.insert(index, default);
        }
        self.get_mut(index).unwrap()
    }

    /// Entry API：获取或使用函数插入
    pub fn get_or_insert_with<F>(&mut self, index: usize, f: F) -> &mut V
    where
        F: FnOnce() -> V,
    {
        if !self.contains(index) {
            self.insert(index, f());
        }
        self.get_mut(index).unwrap()
    }

    /// Entry API：获取或使用默认值插入
    pub fn get_or_default(&mut self, index: usize) -> &mut V
    where
        V: Default,
    {
        self.get_or_insert_with(index, V::default)
    }

    /// 范围迭代（按索引有序）
    pub fn range<R>(&self, range: R) -> impl Iterator<Item = (usize, &V)>
    where
        R: RangeBounds<usize>,
    {
        use core::ops::Bound;

        let start = match range.start_bound() {
            Bound::Included(&s) => s,
            Bound::Excluded(&s) => s.saturating_add(1),
            Bound::Unbounded => 0,
        };

        let end = match range.end_bound() {
            Bound::Included(&e) => e,
            Bound::Excluded(&e) => e.saturating_sub(1),
            Bound::Unbounded => usize::MAX,
        };

        self.iter()
            .skip_while(move |(k, _)| *k < start)
            .take_while(move |(k, _)| *k <= end)
    }

    /// 可变范围迭代（按索引有序）
    pub fn range_mut<R>(&mut self, range: R) -> impl Iterator<Item = (usize, &mut V)>
    where
        R: RangeBounds<usize>,
    {
        use core::ops::Bound;

        let start = match range.start_bound() {
            Bound::Included(&s) => s,
            Bound::Excluded(&s) => s.saturating_add(1),
            Bound::Unbounded => 0,
        };

        let end = match range.end_bound() {
            Bound::Included(&e) => e,
            Bound::Excluded(&e) => e.saturating_sub(1),
            Bound::Unbounded => usize::MAX,
        };

        self.iter_mut()
            .skip_while(move |(k, _)| *k < start)
            .take_while(move |(k, _)| *k <= end)
    }
}

/// 迭代器状态
struct IterState<'a, V> {
    node: &'a Node<V>,
    prefix: usize,
    level: usize,
    index: usize,
}

/// 不可变迭代器
pub struct Iter<'a, V> {
    stack: Vec<IterState<'a, V>>,
    tree: &'a RadixTree<V>,
}

impl<'a, V> Iterator for Iter<'a, V> {
    type Item = (usize, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        // 初始化栈
        if self.stack.is_empty() {
            if let Some(root) = self.tree.root.as_ref() {
                self.stack.push(IterState {
                    node: root,
                    prefix: 0,
                    level: self.tree.height - 1,
                    index: 0,
                });
            } else {
                return None;
            }
        }

        loop {
            // 获取当前状态的副本以避免借用冲突
            let (node, prefix, level, mut index) = {
                let state = self.stack.last()?;
                (state.node, state.prefix, state.level, state.index)
            };

            // 查找下一个非空槽位
            while index < RADIX {
                let i = index;
                index += 1;

                if let Some(slot) = node.get_slot(i) {
                    let key = prefix | (i << (level * SHIFT));

                    match slot {
                        Slot::Value(v) if level == 0 => {
                            // 更新索引
                            self.stack.last_mut().unwrap().index = index;
                            return Some((key, v));
                        }
                        Slot::Node(child) if level > 0 => {
                            // 更新当前索引
                            self.stack.last_mut().unwrap().index = index;
                            // 下降到子节点
                            self.stack.push(IterState {
                                node: child,
                                prefix: key,
                                level: level - 1,
                                index: 0,
                            });
                            break;
                        }
                        _ => {}
                    }
                }
            }

            // 更新索引
            if let Some(state) = self.stack.last_mut() {
                state.index = index;
            }

            // 如果当前层遍历完，回溯
            if self.stack.last().unwrap().index >= RADIX {
                self.stack.pop();
                if self.stack.is_empty() {
                    return None;
                }
            }
        }
    }
}

/// 可变迭代器
pub struct IterMut<'a, V> {
    stack: Vec<(*mut Node<V>, usize, usize, usize)>, // (node, prefix, level, index)
    tree: *mut RadixTree<V>,
    _marker: core::marker::PhantomData<&'a mut V>,
}

impl<'a, V> Iterator for IterMut<'a, V> {
    type Item = (usize, &'a mut V);

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            // 初始化栈
            if self.stack.is_empty() {
                let tree = &mut *self.tree;
                if let Some(root) = tree.root.as_mut() {
                    self.stack.push((
                        root.as_mut() as *mut Node<V>,
                        0,
                        tree.height - 1,
                        0,
                    ));
                } else {
                    return None;
                }
            }

            loop {
                // 获取当前状态
                let (node_ptr, prefix, level, mut index) = *self.stack.last()?;

                // 查找下一个非空槽位
                let mut found = None;
                while index < RADIX {
                    let i = index;
                    index += 1;

                    // 临时借用节点来检查槽位
                    let node = &*node_ptr;
                    if let Some(slot) = node.get_slot(i) {
                        let key = prefix | (i << (level * SHIFT));

                        match slot {
                            Slot::Value(_) if level == 0 => {
                                // 找到值，记录位置
                                found = Some((key, i, true));
                                break;
                            }
                            Slot::Node(child) if level > 0 => {
                                // 找到子节点，记录位置
                                let child_ptr = child.as_ref() as *const Node<V> as *mut Node<V>;
                                found = Some((key, i, false));
                                self.stack.last_mut().unwrap().3 = index;
                                self.stack.push((child_ptr, key, level - 1, 0));
                                break;
                            }
                            _ => {}
                        }
                    }
                }

                // 更新索引
                self.stack.last_mut().unwrap().3 = index;

                // 处理找到的结果
                if let Some((key, i, is_value)) = found {
                    if is_value {
                        // 返回可变引用
                        let node = &mut *node_ptr;
                        if let Some(Slot::Value(v)) = node.get_slot_mut(i) {
                            return Some((key, v));
                        }
                    } else {
                        // 已经下降到子节点，继续循环
                        continue;
                    }
                }

                // 如果当前层遍历完，回溯
                if self.stack.last().unwrap().3 >= RADIX {
                    self.stack.pop();
                    if self.stack.is_empty() {
                        return None;
                    }
                }
            }
        }
    }
}


impl<V> Default for RadixTree<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: Clone> Clone for RadixTree<V> {
    fn clone(&self) -> Self {
        let mut new_tree = Self::new();
        for (k, v) in self.iter() {
            new_tree.insert(k, v.clone());
        }
        new_tree
    }
}

impl<V: core::fmt::Debug> core::fmt::Debug for RadixTree<V> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl<V> FromIterator<(usize, V)> for RadixTree<V> {
    fn from_iter<T: IntoIterator<Item = (usize, V)>>(iter: T) -> Self {
        let mut tree = Self::new();
        for (k, v) in iter {
            tree.insert(k, v);
        }
        tree
    }
}

impl<V> Extend<(usize, V)> for RadixTree<V> {
    fn extend<T: IntoIterator<Item = (usize, V)>>(&mut self, iter: T) {
        for (k, v) in iter {
            self.insert(k, v);
        }
    }
}

impl<'a, V> IntoIterator for &'a RadixTree<V> {
    type Item = (usize, &'a V);
    type IntoIter = Iter<'a, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, V> IntoIterator for &'a mut RadixTree<V> {
    type Item = (usize, &'a mut V);
    type IntoIter = IterMut<'a, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<V> IntoIterator for RadixTree<V> {
    type Item = (usize, V);
    type IntoIter = IntoIter<V>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            items: self.iter().map(|(k, _)| k).collect(),
            tree: self,
            index: 0,
        }
    }
}

/// 消耗式迭代器
pub struct IntoIter<V> {
    items: Vec<usize>,
    tree: RadixTree<V>,
    index: usize,
}

impl<V> Iterator for IntoIter<V> {
    type Item = (usize, V);

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.items.len() {
            return None;
        }

        let key = self.items[self.index];
        self.index += 1;

        self.tree.remove(key).map(|v| (key, v))
    }
}
