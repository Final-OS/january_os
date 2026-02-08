//! 红黑树实现
//!
//! 自平衡二叉搜索树，保证 O(log n) 的插入、删除和查找。
//! 每次旋转操作为 O(1)，适合内核实时性要求。
//!
//! 红黑性质：
//! 1. 每个节点为红色或黑色
//! 2. 根节点为黑色
//! 3. 空叶子（NIL）为黑色
//! 4. 红色节点的子节点必须为黑色
//! 5. 从任意节点到其后代叶子的所有路径包含相同数量的黑色节点

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt;
use core::marker::PhantomData;
use core::ops::{Bound, RangeBounds};
use core::ptr;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Color {
    Red,
    Black,
}

struct RbNode<K, V> {
    key: K,
    value: V,
    left: *mut Self,
    right: *mut Self,
    parent: *mut Self,
    color: Color,
}

impl<K, V> RbNode<K, V> {
    fn new(key: K, value: V) -> *mut Self {
        Box::into_raw(Box::new(RbNode {
            key,
            value,
            left: ptr::null_mut(),
            right: ptr::null_mut(),
            parent: ptr::null_mut(),
            color: Color::Red,
        }))
    }
}

// ---------------------------------------------------------------------------
// 裸指针辅助函数
// ---------------------------------------------------------------------------

#[inline]
unsafe fn color<K, V>(p: *mut RbNode<K, V>) -> Color {
    if p.is_null() { Color::Black } else { (*p).color }
}

#[inline]
unsafe fn set_color<K, V>(p: *mut RbNode<K, V>, c: Color) {
    if !p.is_null() { (*p).color = c; }
}

unsafe fn tree_minimum<K, V>(mut p: *mut RbNode<K, V>) -> *mut RbNode<K, V> {
    while !p.is_null() && !(*p).left.is_null() {
        p = (*p).left;
    }
    p
}

unsafe fn tree_maximum<K, V>(mut p: *mut RbNode<K, V>) -> *mut RbNode<K, V> {
    while !p.is_null() && !(*p).right.is_null() {
        p = (*p).right;
    }
    p
}

unsafe fn successor<K, V>(p: *mut RbNode<K, V>) -> *mut RbNode<K, V> {
    if p.is_null() { return ptr::null_mut(); }
    if !(*p).right.is_null() { return tree_minimum((*p).right); }
    let mut x = p;
    let mut y = (*x).parent;
    while !y.is_null() && x == (*y).right {
        x = y;
        y = (*y).parent;
    }
    y
}

unsafe fn predecessor<K, V>(p: *mut RbNode<K, V>) -> *mut RbNode<K, V> {
    if p.is_null() { return ptr::null_mut(); }
    if !(*p).left.is_null() { return tree_maximum((*p).left); }
    let mut x = p;
    let mut y = (*x).parent;
    while !y.is_null() && x == (*y).left {
        x = y;
        y = (*y).parent;
    }
    y
}

unsafe fn free_subtree<K, V>(p: *mut RbNode<K, V>) {
    if p.is_null() { return; }
    free_subtree((*p).left);
    free_subtree((*p).right);
    let _ = Box::from_raw(p);
}

// ---------------------------------------------------------------------------
// 旋转与修复
// ---------------------------------------------------------------------------

unsafe fn rotate_left<K, V>(root: &mut *mut RbNode<K, V>, x: *mut RbNode<K, V>) {
    let y = (*x).right;
    (*x).right = (*y).left;
    if !(*y).left.is_null() { (*(*y).left).parent = x; }
    (*y).parent = (*x).parent;
    if (*x).parent.is_null() {
        *root = y;
    } else if x == (*(*x).parent).left {
        (*(*x).parent).left = y;
    } else {
        (*(*x).parent).right = y;
    }
    (*y).left = x;
    (*x).parent = y;
}

unsafe fn rotate_right<K, V>(root: &mut *mut RbNode<K, V>, x: *mut RbNode<K, V>) {
    let y = (*x).left;
    (*x).left = (*y).right;
    if !(*y).right.is_null() { (*(*y).right).parent = x; }
    (*y).parent = (*x).parent;
    if (*x).parent.is_null() {
        *root = y;
    } else if x == (*(*x).parent).right {
        (*(*x).parent).right = y;
    } else {
        (*(*x).parent).left = y;
    }
    (*y).right = x;
    (*x).parent = y;
}

unsafe fn transplant<K, V>(
    root: &mut *mut RbNode<K, V>,
    u: *mut RbNode<K, V>,
    v: *mut RbNode<K, V>,
) {
    if (*u).parent.is_null() {
        *root = v;
    } else if u == (*(*u).parent).left {
        (*(*u).parent).left = v;
    } else {
        (*(*u).parent).right = v;
    }
    if !v.is_null() { (*v).parent = (*u).parent; }
}

unsafe fn rb_insert_fixup<K, V>(root: &mut *mut RbNode<K, V>, mut z: *mut RbNode<K, V>) {
    while !(*z).parent.is_null() && color((*z).parent) == Color::Red {
        let p = (*z).parent;
        let gp = (*p).parent;
        if p == (*gp).left {
            let uncle = (*gp).right;
            if color(uncle) == Color::Red {
                set_color(p, Color::Black);
                set_color(uncle, Color::Black);
                set_color(gp, Color::Red);
                z = gp;
            } else {
                if z == (*p).right {
                    z = p;
                    rotate_left(root, z);
                }
                let p = (*z).parent;
                let gp = (*p).parent;
                set_color(p, Color::Black);
                set_color(gp, Color::Red);
                rotate_right(root, gp);
            }
        } else {
            let uncle = (*gp).left;
            if color(uncle) == Color::Red {
                set_color(p, Color::Black);
                set_color(uncle, Color::Black);
                set_color(gp, Color::Red);
                z = gp;
            } else {
                if z == (*p).left {
                    z = p;
                    rotate_right(root, z);
                }
                let p = (*z).parent;
                let gp = (*p).parent;
                set_color(p, Color::Black);
                set_color(gp, Color::Red);
                rotate_left(root, gp);
            }
        }
    }
    set_color(*root, Color::Black);
}

unsafe fn rb_delete_fixup<K, V>(
    root: &mut *mut RbNode<K, V>,
    mut x: *mut RbNode<K, V>,
    mut x_parent: *mut RbNode<K, V>,
) {
    while x != *root && color(x) == Color::Black {
        let p = if !x.is_null() { (*x).parent } else { x_parent };
        if p.is_null() { break; }
        if x == (*p).left {
            let mut w = (*p).right;
            if color(w) == Color::Red {
                set_color(w, Color::Black);
                set_color(p, Color::Red);
                rotate_left(root, p);
                w = (*p).right;
            }
            if color((*w).left) == Color::Black && color((*w).right) == Color::Black {
                set_color(w, Color::Red);
                x = p;
                x_parent = (*p).parent;
            } else {
                if color((*w).right) == Color::Black {
                    set_color((*w).left, Color::Black);
                    set_color(w, Color::Red);
                    rotate_right(root, w);
                    w = (*p).right;
                }
                set_color(w, color(p));
                set_color(p, Color::Black);
                set_color((*w).right, Color::Black);
                rotate_left(root, p);
                x = *root;
                x_parent = ptr::null_mut();
            }
        } else {
            let mut w = (*p).left;
            if color(w) == Color::Red {
                set_color(w, Color::Black);
                set_color(p, Color::Red);
                rotate_right(root, p);
                w = (*p).left;
            }
            if color((*w).right) == Color::Black && color((*w).left) == Color::Black {
                set_color(w, Color::Red);
                x = p;
                x_parent = (*p).parent;
            } else {
                if color((*w).left) == Color::Black {
                    set_color((*w).right, Color::Black);
                    set_color(w, Color::Red);
                    rotate_left(root, w);
                    w = (*p).left;
                }
                set_color(w, color(p));
                set_color(p, Color::Black);
                set_color((*w).left, Color::Black);
                rotate_right(root, p);
                x = *root;
                x_parent = ptr::null_mut();
            }
        }
    }
    set_color(x, Color::Black);
}

// ===========================================================================
// RbTree 公开结构
// ===========================================================================

/// 红黑树有序映射
pub struct RbTree<K, V> {
    root: *mut RbNode<K, V>,
    len: usize,
}

// SAFETY: RbTree 内部无同步机制。Send 安全性依赖于 K/V: Send，
// Sync 安全性要求调用者提供外部同步（如 SpinLock/Mutex）。
unsafe impl<K: Send, V: Send> Send for RbTree<K, V> {}
unsafe impl<K: Sync, V: Sync> Sync for RbTree<K, V> {}

impl<K: Ord, V> RbTree<K, V> {
    /// 创建空树
    pub const fn new() -> Self {
        Self { root: ptr::null_mut(), len: 0 }
    }

    #[inline]
    pub fn len(&self) -> usize { self.len }

    #[inline]
    pub fn is_empty(&self) -> bool { self.len == 0 }

    // -----------------------------------------------------------------------
    // 内部查找
    // -----------------------------------------------------------------------

    unsafe fn find_node(&self, key: &K) -> *mut RbNode<K, V> {
        let mut cur = self.root;
        while !cur.is_null() {
            if *key < (*cur).key {
                cur = (*cur).left;
            } else if *key > (*cur).key {
                cur = (*cur).right;
            } else {
                return cur;
            }
        }
        ptr::null_mut()
    }

    /// key 最大且 <= given 的节点
    unsafe fn floor_node(&self, key: &K) -> *mut RbNode<K, V> {
        let mut result = ptr::null_mut();
        let mut cur = self.root;
        while !cur.is_null() {
            if (*cur).key <= *key {
                result = cur;
                cur = (*cur).right;
            } else {
                cur = (*cur).left;
            }
        }
        result
    }

    /// key 最小且 >= given 的节点
    unsafe fn lower_bound_node(&self, key: &K) -> *mut RbNode<K, V> {
        let mut result = ptr::null_mut();
        let mut cur = self.root;
        while !cur.is_null() {
            if (*cur).key >= *key {
                result = cur;
                cur = (*cur).left;
            } else {
                cur = (*cur).right;
            }
        }
        result
    }

    /// key 最小且 > given 的节点
    unsafe fn upper_bound_node(&self, key: &K) -> *mut RbNode<K, V> {
        let mut result = ptr::null_mut();
        let mut cur = self.root;
        while !cur.is_null() {
            if (*cur).key > *key {
                result = cur;
                cur = (*cur).left;
            } else {
                cur = (*cur).right;
            }
        }
        result
    }

    /// key 最大且 < given 的节点
    unsafe fn lower_than_node(&self, key: &K) -> *mut RbNode<K, V> {
        let mut result = ptr::null_mut();
        let mut cur = self.root;
        while !cur.is_null() {
            if (*cur).key < *key {
                result = cur;
                cur = (*cur).right;
            } else {
                cur = (*cur).left;
            }
        }
        result
    }

    // -----------------------------------------------------------------------
    // 内部插入 / 删除
    // -----------------------------------------------------------------------

    /// BST 插入 + RB 修复
    unsafe fn internal_insert(&mut self, node: *mut RbNode<K, V>) {
        let mut parent_ptr: *mut RbNode<K, V> = ptr::null_mut();
        let mut cur = self.root;
        while !cur.is_null() {
            parent_ptr = cur;
            if (*node).key < (*cur).key {
                cur = (*cur).left;
            } else {
                cur = (*cur).right;
            }
        }
        (*node).parent = parent_ptr;
        (*node).left = ptr::null_mut();
        (*node).right = ptr::null_mut();
        (*node).color = Color::Red;

        if parent_ptr.is_null() {
            self.root = node;
        } else if (*node).key < (*parent_ptr).key {
            (*parent_ptr).left = node;
        } else {
            (*parent_ptr).right = node;
        }
        self.len += 1;
        rb_insert_fixup(&mut self.root, node);
    }

    /// 从树中删除节点，返回 Box
    unsafe fn internal_delete(&mut self, z: *mut RbNode<K, V>) -> Box<RbNode<K, V>> {
        let need_fixup: Color;
        let x: *mut RbNode<K, V>;
        let x_parent: *mut RbNode<K, V>;

        if (*z).left.is_null() {
            need_fixup = (*z).color;
            x = (*z).right;
            x_parent = (*z).parent;
            transplant(&mut self.root, z, (*z).right);
        } else if (*z).right.is_null() {
            need_fixup = (*z).color;
            x = (*z).left;
            x_parent = (*z).parent;
            transplant(&mut self.root, z, (*z).left);
        } else {
            let y = tree_minimum((*z).right);
            need_fixup = (*y).color;
            x = (*y).right;
            if (*y).parent == z {
                x_parent = y;
            } else {
                x_parent = (*y).parent;
                transplant(&mut self.root, y, (*y).right);
                (*y).right = (*z).right;
                (*(*y).right).parent = y;
            }
            transplant(&mut self.root, z, y);
            (*y).left = (*z).left;
            (*(*y).left).parent = y;
            (*y).color = (*z).color;
        }

        self.len -= 1;
        if need_fixup == Color::Black && !self.root.is_null() {
            rb_delete_fixup(&mut self.root, x, x_parent);
        }

        (*z).left = ptr::null_mut();
        (*z).right = ptr::null_mut();
        (*z).parent = ptr::null_mut();
        Box::from_raw(z)
    }

    // -----------------------------------------------------------------------
    // 公开 API — 查询
    // -----------------------------------------------------------------------

    #[inline]
    pub fn contains_key(&self, key: &K) -> bool {
        unsafe { !self.find_node(key).is_null() }
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        unsafe {
            let node = self.find_node(key);
            if node.is_null() { None } else { Some(&(*node).value) }
        }
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        unsafe {
            let node = self.find_node(key);
            if node.is_null() { None } else { Some(&mut (*node).value) }
        }
    }

    pub fn get_key_value(&self, key: &K) -> Option<(&K, &V)> {
        unsafe {
            let node = self.find_node(key);
            if node.is_null() { None } else { Some((&(*node).key, &(*node).value)) }
        }
    }

    pub fn first_key_value(&self) -> Option<(&K, &V)> {
        unsafe {
            let node = tree_minimum(self.root);
            if node.is_null() { None } else { Some((&(*node).key, &(*node).value)) }
        }
    }

    pub fn last_key_value(&self) -> Option<(&K, &V)> {
        unsafe {
            let node = tree_maximum(self.root);
            if node.is_null() { None } else { Some((&(*node).key, &(*node).value)) }
        }
    }

    /// 第一个 key >= given
    pub fn lower_bound(&self, key: &K) -> Option<(&K, &V)> {
        unsafe {
            let node = self.lower_bound_node(key);
            if node.is_null() { None } else { Some((&(*node).key, &(*node).value)) }
        }
    }

    /// 第一个 key > given
    pub fn upper_bound(&self, key: &K) -> Option<(&K, &V)> {
        unsafe {
            let node = self.upper_bound_node(key);
            if node.is_null() { None } else { Some((&(*node).key, &(*node).value)) }
        }
    }

    /// 最后一个 key <= given
    pub fn floor(&self, key: &K) -> Option<(&K, &V)> {
        unsafe {
            let node = self.floor_node(key);
            if node.is_null() { None } else { Some((&(*node).key, &(*node).value)) }
        }
    }

    /// 最后一个 key < given
    pub fn lower_than(&self, key: &K) -> Option<(&K, &V)> {
        unsafe {
            let node = self.lower_than_node(key);
            if node.is_null() { None } else { Some((&(*node).key, &(*node).value)) }
        }
    }

    // -----------------------------------------------------------------------
    // 公开 API — 插入 / 删除
    // -----------------------------------------------------------------------

    /// 插入键值对。若键已存在，替换值并返回旧值。
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        unsafe {
            let existing = self.find_node(&key);
            if !existing.is_null() {
                let old = core::ptr::read(&(*existing).value);
                core::ptr::write(&mut (*existing).value, value);
                return Some(old);
            }
            let node = RbNode::new(key, value);
            self.internal_insert(node);
            None
        }
    }

    /// 删除并返回值
    pub fn remove(&mut self, key: &K) -> Option<V> {
        unsafe {
            let node = self.find_node(key);
            if node.is_null() { return None; }
            let boxed = self.internal_delete(node);
            Some(boxed.value)
        }
    }

    /// 删除并返回键值对
    pub fn remove_entry(&mut self, key: &K) -> Option<(K, V)> {
        unsafe {
            let node = self.find_node(key);
            if node.is_null() { return None; }
            let boxed = self.internal_delete(node);
            Some((boxed.key, boxed.value))
        }
    }

    pub fn clear(&mut self) {
        unsafe { free_subtree(self.root); }
        self.root = ptr::null_mut();
        self.len = 0;
    }

    /// 追加另一棵树的所有元素（同键覆盖）
    pub fn append(&mut self, other: &mut Self) {
        while let Some((k, v)) = other.pop_first() {
            self.insert(k, v);
        }
    }

    pub fn pop_first(&mut self) -> Option<(K, V)> {
        unsafe {
            let node = tree_minimum(self.root);
            if node.is_null() { return None; }
            let boxed = self.internal_delete(node);
            Some((boxed.key, boxed.value))
        }
    }

    pub fn pop_last(&mut self) -> Option<(K, V)> {
        unsafe {
            let node = tree_maximum(self.root);
            if node.is_null() { return None; }
            let boxed = self.internal_delete(node);
            Some((boxed.key, boxed.value))
        }
    }

    /// 保留满足条件的元素
    pub fn retain<F: FnMut(&K, &mut V) -> bool>(&mut self, mut f: F) {
        let mut nodes_to_remove: Vec<*mut RbNode<K, V>> = Vec::new();
        unsafe {
            let mut node = tree_minimum(self.root);
            while !node.is_null() {
                let next = successor(node);
                if !f(&(*node).key, &mut (*node).value) {
                    nodes_to_remove.push(node);
                }
                node = next;
            }
        }
        for node in nodes_to_remove {
            unsafe { self.internal_delete(node); }
        }
    }

    /// Entry API：按键查找或准备插入
    pub fn entry(&mut self, key: K) -> Entry<'_, K, V> {
        unsafe {
            let node = self.find_node(&key);
            if node.is_null() {
                Entry::Vacant(VacantEntry { key, tree: self })
            } else {
                Entry::Occupied(OccupiedEntry { node, tree: self })
            }
        }
    }

    // -----------------------------------------------------------------------
    // 迭代器
    // -----------------------------------------------------------------------

    pub fn iter(&self) -> RbIter<'_, K, V> {
        RbIter {
            current: unsafe { tree_minimum(self.root) },
            _marker: PhantomData,
        }
    }

    pub fn iter_mut(&mut self) -> RbIterMut<'_, K, V> {
        RbIterMut {
            current: unsafe { tree_minimum(self.root) },
            _marker: PhantomData,
        }
    }

    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.iter().map(|(k, _)| k)
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.iter().map(|(_, v)| v)
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.iter_mut().map(|(_, v)| v)
    }

    /// 范围迭代
    pub fn range<R: RangeBounds<K>>(&self, range: R) -> RbRangeIter<'_, K, V> {
        let start_node = match range.start_bound() {
            Bound::Included(k) => unsafe { self.lower_bound_node(k) },
            Bound::Excluded(k) => unsafe { self.upper_bound_node(k) },
            Bound::Unbounded => unsafe { tree_minimum(self.root) },
        };
        let end_bound = match range.end_bound() {
            Bound::Included(k) => RangeEnd::Included(k as *const K),
            Bound::Excluded(k) => RangeEnd::Excluded(k as *const K),
            Bound::Unbounded => RangeEnd::Unbounded,
        };
        RbRangeIter {
            current: start_node,
            end_bound,
            _marker: PhantomData,
        }
    }

    /// 可变范围迭代
    pub fn range_mut<R: RangeBounds<K>>(&mut self, range: R) -> RbRangeIterMut<'_, K, V> {
        let start_node = match range.start_bound() {
            Bound::Included(k) => unsafe { self.lower_bound_node(k) },
            Bound::Excluded(k) => unsafe { self.upper_bound_node(k) },
            Bound::Unbounded => unsafe { tree_minimum(self.root) },
        };
        let end_bound = match range.end_bound() {
            Bound::Included(k) => RangeEnd::Included(k as *const K),
            Bound::Excluded(k) => RangeEnd::Excluded(k as *const K),
            Bound::Unbounded => RangeEnd::Unbounded,
        };
        RbRangeIterMut {
            current: start_node,
            end_bound,
            _marker: PhantomData,
        }
    }
}

// ===========================================================================
// Entry API
// ===========================================================================

pub enum Entry<'a, K: Ord, V> {
    Occupied(OccupiedEntry<'a, K, V>),
    Vacant(VacantEntry<'a, K, V>),
}

pub struct OccupiedEntry<'a, K: Ord, V> {
    node: *mut RbNode<K, V>,
    tree: &'a mut RbTree<K, V>,
}

pub struct VacantEntry<'a, K: Ord, V> {
    key: K,
    tree: &'a mut RbTree<K, V>,
}

impl<'a, K: Ord, V> Entry<'a, K, V> {
    pub fn or_insert(self, default: V) -> &'a mut V {
        match self {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(default),
        }
    }

    pub fn or_insert_with<F: FnOnce() -> V>(self, f: F) -> &'a mut V {
        match self {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(f()),
        }
    }

    pub fn and_modify<F: FnOnce(&mut V)>(self, f: F) -> Self {
        match self {
            Entry::Occupied(mut e) => {
                f(e.get_mut());
                Entry::Occupied(e)
            }
            Entry::Vacant(e) => Entry::Vacant(e),
        }
    }
}

impl<'a, K: Ord, V> OccupiedEntry<'a, K, V> {
    pub fn key(&self) -> &K { unsafe { &(*self.node).key } }
    pub fn get(&self) -> &V { unsafe { &(*self.node).value } }
    pub fn get_mut(&mut self) -> &mut V { unsafe { &mut (*self.node).value } }
    pub fn into_mut(self) -> &'a mut V { unsafe { &mut (*self.node).value } }

    pub fn insert(&mut self, value: V) -> V {
        unsafe {
            let old = core::ptr::read(&(*self.node).value);
            core::ptr::write(&mut (*self.node).value, value);
            old
        }
    }

    pub fn remove(self) -> V {
        unsafe { self.tree.internal_delete(self.node).value }
    }
}

impl<'a, K: Ord, V> VacantEntry<'a, K, V> {
    pub fn key(&self) -> &K { &self.key }

    pub fn insert(self, value: V) -> &'a mut V {
        let node = RbNode::new(self.key, value);
        unsafe {
            self.tree.internal_insert(node);
            &mut (*node).value
        }
    }
}

// ===========================================================================
// 迭代器
// ===========================================================================

/// 范围终止条件（内部使用）
enum RangeEnd<K> {
    Included(*const K),
    Excluded(*const K),
    Unbounded,
}

pub struct RbIter<'a, K, V> {
    current: *mut RbNode<K, V>,
    _marker: PhantomData<&'a (K, V)>,
}

impl<'a, K, V> Iterator for RbIter<'a, K, V> {
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        if self.current.is_null() { return None; }
        unsafe {
            let n = &*self.current;
            self.current = successor(self.current);
            Some((&n.key, &n.value))
        }
    }
}

pub struct RbIterMut<'a, K, V> {
    current: *mut RbNode<K, V>,
    _marker: PhantomData<&'a mut (K, V)>,
}

impl<'a, K, V> Iterator for RbIterMut<'a, K, V> {
    type Item = (&'a K, &'a mut V);
    fn next(&mut self) -> Option<Self::Item> {
        if self.current.is_null() { return None; }
        unsafe {
            let n = &mut *self.current;
            self.current = successor(self.current);
            Some((&n.key, &mut n.value))
        }
    }
}

pub struct RbRangeIter<'a, K, V> {
    current: *mut RbNode<K, V>,
    end_bound: RangeEnd<K>,
    _marker: PhantomData<&'a (K, V)>,
}

impl<'a, K: Ord, V> Iterator for RbRangeIter<'a, K, V> {
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        if self.current.is_null() { return None; }
        unsafe {
            let n = &*self.current;
            match &self.end_bound {
                RangeEnd::Included(end) => {
                    if n.key > **end { return None; }
                }
                RangeEnd::Excluded(end) => {
                    if n.key >= **end { return None; }
                }
                RangeEnd::Unbounded => {}
            }
            self.current = successor(self.current);
            Some((&n.key, &n.value))
        }
    }
}

pub struct RbRangeIterMut<'a, K, V> {
    current: *mut RbNode<K, V>,
    end_bound: RangeEnd<K>,
    _marker: PhantomData<&'a mut (K, V)>,
}

impl<'a, K: Ord, V> Iterator for RbRangeIterMut<'a, K, V> {
    type Item = (&'a K, &'a mut V);
    fn next(&mut self) -> Option<Self::Item> {
        if self.current.is_null() { return None; }
        unsafe {
            let n = &mut *self.current;
            match &self.end_bound {
                RangeEnd::Included(end) => {
                    if n.key > **end { return None; }
                }
                RangeEnd::Excluded(end) => {
                    if n.key >= **end { return None; }
                }
                RangeEnd::Unbounded => {}
            }
            self.current = successor(self.current);
            Some((&n.key, &mut n.value))
        }
    }
}

// ===========================================================================
// Trait 实现
// ===========================================================================

impl<K, V> Drop for RbTree<K, V> {
    fn drop(&mut self) {
        unsafe { free_subtree(self.root); }
        self.root = ptr::null_mut();
    }
}

impl<K: Ord, V> Default for RbTree<K, V> {
    fn default() -> Self { Self::new() }
}

impl<K: Ord + Clone, V: Clone> Clone for RbTree<K, V> {
    fn clone(&self) -> Self {
        let mut new_tree = Self::new();
        for (k, v) in self.iter() {
            new_tree.insert(k.clone(), v.clone());
        }
        new_tree
    }
}

impl<K: Ord + fmt::Debug, V: fmt::Debug> fmt::Debug for RbTree<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}
