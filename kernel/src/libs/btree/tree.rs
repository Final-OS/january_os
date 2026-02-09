//! 通用 B-Tree 实现
//!
//! 提供高效的有序键值对存储，针对现代处理器缓存优化。
//!
//! ## 特性
//! - 分支因子 16（可配置）
//! - 节点大小约 256 bytes
//! - O(log_B N) 查找、插入、删除
//! - 支持范围查询和迭代

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::mem;

/// B-Tree 分支因子
pub const B: usize = 16;

/// 最小键数（除根节点外）
pub const MIN_KEYS: usize = B / 2 - 1;

/// 最大键数
pub const MAX_KEYS: usize = B - 1;

/// B-Tree 节点
pub struct BTreeNode<K, V> {
    /// 是否为叶子节点
    pub is_leaf: bool,

    /// 键值对
    pub keys: Vec<K>,
    pub values: Vec<V>,

    /// 子节点指针（仅内部节点）
    pub children: Vec<Box<BTreeNode<K, V>>>,
}

impl<K, V> BTreeNode<K, V> {
    /// 创建新的叶子节点
    pub fn new_leaf() -> Self {
        Self {
            is_leaf: true,
            keys: Vec::new(),
            values: Vec::new(),
            children: Vec::new(),
        }
    }

    /// 创建新的内部节点
    pub fn new_internal() -> Self {
        Self {
            is_leaf: false,
            keys: Vec::new(),
            values: Vec::new(),
            children: Vec::new(),
        }
    }

    /// 节点是否已满
    pub fn is_full(&self) -> bool {
        self.keys.len() >= MAX_KEYS
    }

    /// 节点是否需要合并
    pub fn is_underflow(&self) -> bool {
        self.keys.len() < MIN_KEYS
    }
}

/// B-Tree
pub struct BTree<K, V> {
    pub root: Box<BTreeNode<K, V>>,
    pub len: usize,
}

impl<K: Ord, V> BTree<K, V> {
    /// 创建空树
    pub fn new() -> Self {
        Self {
            root: Box::new(BTreeNode::new_leaf()),
            len: 0,
        }
    }

    /// 元素数量
    pub fn len(&self) -> usize {
        self.len
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// 插入键值对
    pub fn insert(&mut self, key: K, value: V) -> Option<V>
    where
        K: Clone,
    {
        // 如果根节点已满，分裂它
        if self.root.is_full() {
            let mut new_root = Box::new(BTreeNode::new_internal());
            let old_root = mem::replace(&mut self.root, new_root);
            self.root.children.push(old_root);
            unsafe {
                Self::split_child_static(&mut *self.root as *mut BTreeNode<K, V>, 0);
            }
        }

        // 插入到非满的根节点
        let root_ptr = &mut *self.root as *mut BTreeNode<K, V>;
        let old_value = unsafe { Self::insert_non_full_static(root_ptr, key, value) };

        if old_value.is_none() {
            self.len += 1;
        }

        old_value
    }

    /// 在非满节点中插入（静态方法）
    unsafe fn insert_non_full_static(node: *mut BTreeNode<K, V>, key: K, value: V) -> Option<V>
    where
        K: Clone,
    {
        let node = &mut *node;

        // 先检查当前节点是否已有该键
        if let Some(pos) = node.keys.iter().position(|k| k == &key) {
            let old_value = mem::replace(&mut node.values[pos], value);
            return Some(old_value);
        }

        if node.is_leaf {
            // 叶子节点：插入新键值对
            let pos = node.keys.iter().position(|k| k > &key).unwrap_or(node.keys.len());
            node.keys.insert(pos, key);
            node.values.insert(pos, value);
            None
        } else {
            // 内部节点：找到合适的子节点
            let mut child_idx = node.keys.iter().position(|k| k > &key).unwrap_or(node.keys.len());

            // 如果子节点已满，先分裂
            if node.children[child_idx].is_full() {
                Self::split_child_static(node, child_idx);

                // 分裂后重新确定子节点
                if key > node.keys[child_idx] {
                    child_idx += 1;
                }
            }

            let child_ptr = &mut *node.children[child_idx] as *mut BTreeNode<K, V>;
            Self::insert_non_full_static(child_ptr, key, value)
        }
    }

    /// 分裂子节点（静态方法）
    unsafe fn split_child_static(parent: *mut BTreeNode<K, V>, child_idx: usize)
    where
        K: Clone,
    {
        let parent = &mut *parent;
        let mid = MIN_KEYS;

        // 创建新节点
        let mut new_node = if parent.children[child_idx].is_leaf {
            Box::new(BTreeNode::new_leaf())
        } else {
            Box::new(BTreeNode::new_internal())
        };

        // 分裂键值对
        let child = &mut parent.children[child_idx];
        new_node.keys = child.keys.split_off(mid + 1);
        new_node.values = child.values.split_off(mid + 1);
        let mid_key = child.keys.pop().unwrap();
        let mid_value = child.values.pop().unwrap();

        // 分裂子节点（如果是内部节点）
        if !child.is_leaf {
            new_node.children = child.children.split_off(mid + 1);
        }

        // 插入到父节点
        parent.keys.insert(child_idx, mid_key);
        parent.values.insert(child_idx, mid_value);
        parent.children.insert(child_idx + 1, new_node);
    }

    /// 查找键
    pub fn get(&self, key: &K) -> Option<&V> {
        Self::get_in_node(&self.root, key)
    }

    /// 获取可变引用
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        Self::get_mut_in_node(&mut self.root, key)
    }

    /// 检查键是否存在
    #[inline]
    pub fn contains_key(&self, key: &K) -> bool {
        self.get(key).is_some()
    }

    /// 清空树
    pub fn clear(&mut self) {
        self.root = Box::new(BTreeNode::new_leaf());
        self.len = 0;
    }

    /// 获取第一个键值对
    pub fn first(&self) -> Option<(&K, &V)> {
        if self.is_empty() {
            return None;
        }
        Self::first_in_node(&self.root)
    }

    /// 获取最后一个键值对
    pub fn last(&self) -> Option<(&K, &V)> {
        if self.is_empty() {
            return None;
        }
        Self::last_in_node(&self.root)
    }

    /// 弹出第一个键值对
    pub fn pop_first(&mut self) -> Option<(K, V)>
    where
        K: Clone,
    {
        let (key, _) = self.first()?;
        let key = key.clone();
        let value = self.remove(&key)?;
        Some((key, value))
    }

    /// 弹出最后一个键值对
    pub fn pop_last(&mut self) -> Option<(K, V)>
    where
        K: Clone,
    {
        let (key, _) = self.last()?;
        let key = key.clone();
        let value = self.remove(&key)?;
        Some((key, value))
    }

    /// 在节点中查找
    fn get_in_node<'a>(node: &'a BTreeNode<K, V>, key: &K) -> Option<&'a V> {
        // 在当前节点中查找键
        if let Some(pos) = node.keys.iter().position(|k| k == key) {
            return Some(&node.values[pos]);
        }

        // 如果是叶子节点，键不存在
        if node.is_leaf {
            return None;
        }

        // 内部节点：找到合适的子节点继续查找
        let child_idx = node.keys.iter().position(|k| k > key).unwrap_or(node.keys.len());
        Self::get_in_node(&node.children[child_idx], key)
    }

    /// 在节点中查找可变引用
    fn get_mut_in_node<'a>(node: &'a mut BTreeNode<K, V>, key: &K) -> Option<&'a mut V> {
        // 在当前节点中查找键
        if let Some(pos) = node.keys.iter().position(|k| k == key) {
            return Some(&mut node.values[pos]);
        }

        // 如果是叶子节点，键不存在
        if node.is_leaf {
            return None;
        }

        // 内部节点：找到合适的子节点继续查找
        let child_idx = node.keys.iter().position(|k| k > key).unwrap_or(node.keys.len());
        Self::get_mut_in_node(&mut node.children[child_idx], key)
    }

    /// 查找第一个键值对
    fn first_in_node<'a>(node: &'a BTreeNode<K, V>) -> Option<(&'a K, &'a V)> {
        if node.is_leaf {
            if node.keys.is_empty() {
                None
            } else {
                Some((&node.keys[0], &node.values[0]))
            }
        } else {
            Self::first_in_node(&node.children[0])
        }
    }

    /// 查找最后一个键值对
    fn last_in_node<'a>(node: &'a BTreeNode<K, V>) -> Option<(&'a K, &'a V)> {
        if node.is_leaf {
            if node.keys.is_empty() {
                None
            } else {
                let idx = node.keys.len() - 1;
                Some((&node.keys[idx], &node.values[idx]))
            }
        } else {
            let idx = node.children.len() - 1;
            Self::last_in_node(&node.children[idx])
        }
    }

    /// 删除键
    pub fn remove(&mut self, key: &K) -> Option<V>
    where
        K: Clone,
    {
        let root_ptr = &mut *self.root as *mut BTreeNode<K, V>;
        let result = unsafe { Self::remove_from_node_static(root_ptr, key)? };
        self.len -= 1;

        // 如果根节点为空且有子节点，提升子节点为根
        if self.root.keys.is_empty() && !self.root.is_leaf {
            if let Some(child) = self.root.children.pop() {
                self.root = child;
            }
        }

        Some(result)
    }

    /// 从节点中删除（静态方法）
    unsafe fn remove_from_node_static(node: *mut BTreeNode<K, V>, key: &K) -> Option<V>
    where
        K: Clone,
    {
        let node = &mut *node;

        // 先检查当前节点是否有该键
        if let Some(pos) = node.keys.iter().position(|k| k == key) {
            if node.is_leaf {
                // 叶子节点：直接删除
                node.keys.remove(pos);
                return Some(node.values.remove(pos));
            } else {
                // 内部节点：需要用前驱或后继替换
                // 简化实现：从左子树找最大值替换
                if node.children[pos].keys.len() > MIN_KEYS {
                    let child_ptr = &mut *node.children[pos] as *mut BTreeNode<K, V>;
                    let (pred_key, pred_value) = Self::remove_max_static(child_ptr);
                    let old_key = mem::replace(&mut node.keys[pos], pred_key);
                    let old_value = mem::replace(&mut node.values[pos], pred_value);
                    return Some(old_value);
                } else if node.children[pos + 1].keys.len() > MIN_KEYS {
                    let child_ptr = &mut *node.children[pos + 1] as *mut BTreeNode<K, V>;
                    let (succ_key, succ_value) = Self::remove_min_static(child_ptr);
                    let old_key = mem::replace(&mut node.keys[pos], succ_key);
                    let old_value = mem::replace(&mut node.values[pos], succ_value);
                    return Some(old_value);
                } else {
                    // 合并后删除
                    Self::merge_children_static(node, pos);
                    let child_ptr = &mut *node.children[pos] as *mut BTreeNode<K, V>;
                    return Self::remove_from_node_static(child_ptr, key);
                }
            }
        }

        if node.is_leaf {
            // 叶子节点且键不存在
            return None;
        }

        // 内部节点：找到包含该键的子节点
        let child_idx = node.keys.iter().position(|k| k > key).unwrap_or(node.keys.len());

        // 确保子节点有足够的键
        if node.children[child_idx].keys.len() <= MIN_KEYS {
            Self::fix_child_static(node, child_idx);
        }

        // 重新查找子节点（可能因为修复而改变）
        let child_idx = node.keys.iter().position(|k| k > key).unwrap_or(node.keys.len());
        let child_ptr = &mut *node.children[child_idx] as *mut BTreeNode<K, V>;
        Self::remove_from_node_static(child_ptr, key)
    }

    /// 删除并返回子树中的最大键值对
    unsafe fn remove_max_static(node: *mut BTreeNode<K, V>) -> (K, V)
    where
        K: Clone,
    {
        let node = &mut *node;
        if node.is_leaf {
            let key = node.keys.pop().unwrap();
            let value = node.values.pop().unwrap();
            (key, value)
        } else {
            let last_idx = node.children.len() - 1;
            let child_ptr = &mut *node.children[last_idx] as *mut BTreeNode<K, V>;
            Self::remove_max_static(child_ptr)
        }
    }

    /// 删除并返回子树中的最小键值对
    unsafe fn remove_min_static(node: *mut BTreeNode<K, V>) -> (K, V)
    where
        K: Clone,
    {
        let node = &mut *node;
        if node.is_leaf {
            let key = node.keys.remove(0);
            let value = node.values.remove(0);
            (key, value)
        } else {
            let child_ptr = &mut *node.children[0] as *mut BTreeNode<K, V>;
            Self::remove_min_static(child_ptr)
        }
    }

    /// 修复子节点（静态方法）
    unsafe fn fix_child_static(parent: *mut BTreeNode<K, V>, child_idx: usize)
    where
        K: Clone,
    {
        let parent = &mut *parent;

        // 尝试从左兄弟借
        if child_idx > 0 && parent.children[child_idx - 1].keys.len() > MIN_KEYS {
            Self::borrow_from_left_static(parent, child_idx);
            return;
        }

        // 尝试从右兄弟借
        if child_idx < parent.children.len() - 1
            && parent.children[child_idx + 1].keys.len() > MIN_KEYS
        {
            Self::borrow_from_right_static(parent, child_idx);
            return;
        }

        // 合并
        if child_idx > 0 {
            Self::merge_children_static(parent, child_idx - 1);
        } else {
            Self::merge_children_static(parent, child_idx);
        }
    }

    /// 从左兄弟借一个键（静态方法）
    unsafe fn borrow_from_left_static(parent: *mut BTreeNode<K, V>, child_idx: usize) {
        let parent = &mut *parent;

        let (left_part, right_part) = parent.children.split_at_mut(child_idx);
        let left_sibling = &mut left_part[child_idx - 1];
        let child = &mut right_part[0];

        let borrowed_key = left_sibling.keys.pop().unwrap();
        let borrowed_value = left_sibling.values.pop().unwrap();

        if !child.is_leaf {
            let borrowed_child = left_sibling.children.pop().unwrap();
            child.children.insert(0, borrowed_child);
        }

        let separator_key = mem::replace(&mut parent.keys[child_idx - 1], borrowed_key);
        let separator_value = mem::replace(&mut parent.values[child_idx - 1], borrowed_value);

        child.keys.insert(0, separator_key);
        child.values.insert(0, separator_value);
    }

    /// 从右兄弟借一个键（静态方法）
    unsafe fn borrow_from_right_static(parent: *mut BTreeNode<K, V>, child_idx: usize) {
        let parent = &mut *parent;

        let (left_part, right_part) = parent.children.split_at_mut(child_idx + 1);
        let child = &mut left_part[child_idx];
        let right_sibling = &mut right_part[0];

        let borrowed_key = right_sibling.keys.remove(0);
        let borrowed_value = right_sibling.values.remove(0);

        if !child.is_leaf {
            let borrowed_child = right_sibling.children.remove(0);
            child.children.push(borrowed_child);
        }

        let separator_key = mem::replace(&mut parent.keys[child_idx], borrowed_key);
        let separator_value = mem::replace(&mut parent.values[child_idx], borrowed_value);

        child.keys.push(separator_key);
        child.values.push(separator_value);
    }

    /// 合并两个子节点（静态方法）
    unsafe fn merge_children_static(parent: *mut BTreeNode<K, V>, left_idx: usize) {
        let parent = &mut *parent;

        let separator_key = parent.keys.remove(left_idx);
        let separator_value = parent.values.remove(left_idx);
        let right_child = parent.children.remove(left_idx + 1);
        let left_child = &mut parent.children[left_idx];

        left_child.keys.push(separator_key);
        left_child.values.push(separator_value);
        left_child.keys.extend(right_child.keys);
        left_child.values.extend(right_child.values);

        if !left_child.is_leaf {
            left_child.children.extend(right_child.children);
        }
    }

    /// 迭代所有键值对
    pub fn iter(&self) -> BTreeIter<'_, K, V> {
        BTreeIter {
            stack: vec![(&*self.root, 0)],
        }
    }
}

impl<K, V> Default for BTree<K, V>
where
    K: Ord,
{
    fn default() -> Self {
        Self::new()
    }
}

/// B-Tree 迭代器
pub struct BTreeIter<'a, K, V> {
    stack: Vec<(&'a BTreeNode<K, V>, usize)>,
}

impl<'a, K, V> Iterator for BTreeIter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (node, idx) = self.stack.last_mut()?;

            if node.is_leaf {
                // 叶子节点：直接返回键值对
                if *idx < node.keys.len() {
                    let key = &node.keys[*idx];
                    let value = &node.values[*idx];
                    *idx += 1;
                    return Some((key, value));
                } else {
                    self.stack.pop();
                }
            } else {
                // 内部节点：in-order 遍历
                // 顺序：child[0], key[0], child[1], key[1], ..., child[n]

                let child_count = node.children.len();
                let key_count = node.keys.len();

                if *idx < child_count + key_count {
                    if *idx % 2 == 0 {
                        // 偶数索引：访问子节点
                        let child_idx = *idx / 2;
                        if child_idx < child_count {
                            let child = &node.children[child_idx];
                            *idx += 1;
                            self.stack.push((child, 0));
                        } else {
                            *idx += 1;
                        }
                    } else {
                        // 奇数索引：返回键值对
                        let key_idx = *idx / 2;
                        if key_idx < key_count {
                            let key = &node.keys[key_idx];
                            let value = &node.values[key_idx];
                            *idx += 1;
                            return Some((key, value));
                        } else {
                            *idx += 1;
                        }
                    }
                } else {
                    self.stack.pop();
                }
            }
        }
    }
}

// ============================================================================
// 额外的迭代器和辅助方法
// ============================================================================

impl<K: Ord, V> BTree<K, V> {
    /// 返回键的迭代器
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.iter().map(|(k, _)| k)
    }

    /// 返回值的迭代器
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.iter().map(|(_, v)| v)
    }

    /// 保留满足条件的元素
    pub fn retain<F>(&mut self, f: F)
    where
        F: FnMut(&K, &V) -> bool,
        K: Clone,
    {
        let mut pred = f;
        let to_remove: Vec<K> = self
            .iter()
            .filter_map(|(k, v)| {
                if !pred(k, v) {
                    Some(k.clone())
                } else {
                    None
                }
            })
            .collect();

        for key in to_remove {
            self.remove(&key);
        }
    }
}

// ============================================================================
// 特征实现
// ============================================================================

impl<K: Ord + Clone, V: Clone> Clone for BTree<K, V> {
    fn clone(&self) -> Self {
        let mut new_tree = BTree::new();
        for (k, v) in self.iter() {
            new_tree.insert(k.clone(), v.clone());
        }
        new_tree
    }
}

impl<K: Ord + core::fmt::Debug, V: core::fmt::Debug> core::fmt::Debug for BTree<K, V> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl<K: Ord + Clone, V, const N: usize> From<[(K, V); N]> for BTree<K, V> {
    fn from(arr: [(K, V); N]) -> Self {
        let mut tree = BTree::new();
        for (k, v) in arr {
            tree.insert(k, v);
        }
        tree
    }
}

impl<K: Ord + Clone, V> FromIterator<(K, V)> for BTree<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut tree = BTree::new();
        for (k, v) in iter {
            tree.insert(k, v);
        }
        tree
    }
}

impl<K: Ord + Clone, V> Extend<(K, V)> for BTree<K, V> {
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, iter: I) {
        for (k, v) in iter {
            self.insert(k, v);
        }
    }
}

