//! Maple Tree — 增强红黑区间树实现
//!
//! 基于自定义增强红黑树的不重叠区间映射。每个节点维护 `max_end`（子树最大端点）
//! 和 `max_gap`（子树最大间隙），支持 O(log n) 的间隙搜索。
//!
//! 区间约定为 `[start, end)`，不允许重叠。

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt;
use core::marker::PhantomData;
use core::ops::{Bound, RangeBounds};
use core::ptr;

/// 区间插入错误
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapleInsertError {
    InvalidRange,
    Overlap,
}

/// 节点颜色
#[derive(Clone, Copy, PartialEq, Eq)]
enum Color {
    Red,
    Black,
}

/// 红黑树节点
struct MapleNode<V> {
    start: usize,
    end: usize,
    value: V,
    left: *mut Self,
    right: *mut Self,
    parent: *mut Self,
    color: Color,
    /// 子树中最大的 end 值
    max_end: usize,
    /// 子树中最大的间隙
    max_gap: usize,
    /// 本节点 start 与前驱 end 之间的间隙
    gap_before: usize,
    /// 间隙起始地址（前驱的 end，首节点为 0）
    gap_start: usize,
}

impl<V> MapleNode<V> {
    fn new(start: usize, end: usize, value: V) -> *mut Self {
        let node = Box::new(MapleNode {
            start,
            end,
            value,
            left: ptr::null_mut(),
            right: ptr::null_mut(),
            parent: ptr::null_mut(),
            color: Color::Red,
            max_end: end,
            max_gap: 0,
            gap_before: 0,
            gap_start: 0,
        });
        Box::into_raw(node)
    }
}

/// 增强红黑区间树
///
/// 使用 `[start, end)` 半开区间，不允许重叠。内部维护增强字段以支持
/// O(log n) 的间隙搜索和重叠检测。
pub struct MapleTree<V> {
    root: *mut MapleNode<V>,
    len: usize,
}

// SAFETY: MapleTree 内部无同步机制。Send 安全性依赖于 V: Send，
// Sync 安全性要求调用者提供外部同步（如 SpinLock/Mutex）来保护并发访问。
unsafe impl<V: Send> Send for MapleTree<V> {}
unsafe impl<V: Sync> Sync for MapleTree<V> {}

// ---------------------------------------------------------------------------
// 内部辅助函数（裸指针操作）
// ---------------------------------------------------------------------------

#[inline]
fn is_null<V>(p: *mut MapleNode<V>) -> bool {
    p.is_null()
}

#[inline]
unsafe fn color<V>(p: *mut MapleNode<V>) -> Color {
    if is_null(p) {
        Color::Black
    } else {
        (*p).color
    }
}

#[inline]
unsafe fn set_color<V>(p: *mut MapleNode<V>, c: Color) {
    if !is_null(p) {
        (*p).color = c;
    }
}

#[inline]
unsafe fn left<V>(p: *mut MapleNode<V>) -> *mut MapleNode<V> {
    if is_null(p) {
        ptr::null_mut()
    } else {
        (*p).left
    }
}

#[inline]
unsafe fn right<V>(p: *mut MapleNode<V>) -> *mut MapleNode<V> {
    if is_null(p) {
        ptr::null_mut()
    } else {
        (*p).right
    }
}

#[inline]
unsafe fn parent<V>(p: *mut MapleNode<V>) -> *mut MapleNode<V> {
    if is_null(p) {
        ptr::null_mut()
    } else {
        (*p).parent
    }
}

/// 求子树最小节点
unsafe fn tree_minimum<V>(mut p: *mut MapleNode<V>) -> *mut MapleNode<V> {
    while !is_null(p) && !is_null((*p).left) {
        p = (*p).left;
    }
    p
}

/// 求子树最大节点
unsafe fn tree_maximum<V>(mut p: *mut MapleNode<V>) -> *mut MapleNode<V> {
    while !is_null(p) && !is_null((*p).right) {
        p = (*p).right;
    }
    p
}

/// 中序后继
unsafe fn successor<V>(p: *mut MapleNode<V>) -> *mut MapleNode<V> {
    if is_null(p) {
        return ptr::null_mut();
    }
    if !is_null((*p).right) {
        return tree_minimum((*p).right);
    }
    let mut x = p;
    let mut y = (*x).parent;
    while !is_null(y) && x == (*y).right {
        x = y;
        y = (*y).parent;
    }
    y
}

/// 中序前驱
unsafe fn predecessor<V>(p: *mut MapleNode<V>) -> *mut MapleNode<V> {
    if is_null(p) {
        return ptr::null_mut();
    }
    if !is_null((*p).left) {
        return tree_maximum((*p).left);
    }
    let mut x = p;
    let mut y = (*x).parent;
    while !is_null(y) && x == (*y).left {
        x = y;
        y = (*y).parent;
    }
    y
}

/// 更新单个节点的增强字段（max_end, max_gap）
unsafe fn update_augmented<V>(p: *mut MapleNode<V>) {
    if is_null(p) {
        return;
    }
    let n = &mut *p;
    n.max_end = n.end;
    n.max_gap = n.gap_before;

    if !is_null(n.left) {
        let l = &*n.left;
        if l.max_end > n.max_end {
            n.max_end = l.max_end;
        }
        if l.max_gap > n.max_gap {
            n.max_gap = l.max_gap;
        }
    }
    if !is_null(n.right) {
        let r = &*n.right;
        if r.max_end > n.max_end {
            n.max_end = r.max_end;
        }
        if r.max_gap > n.max_gap {
            n.max_gap = r.max_gap;
        }
    }
}

/// 从节点向上传播增强字段直到根
unsafe fn propagate_augmented<V>(mut p: *mut MapleNode<V>) {
    while !is_null(p) {
        update_augmented(p);
        p = (*p).parent;
    }
}

/// 递归释放子树所有节点
unsafe fn free_subtree<V>(p: *mut MapleNode<V>) {
    if is_null(p) {
        return;
    }
    free_subtree((*p).left);
    free_subtree((*p).right);
    let _ = Box::from_raw(p);
}

// ---------------------------------------------------------------------------
// 红黑树旋转
// ---------------------------------------------------------------------------

/// 左旋：x 的右子 y 上升，x 下降为 y 的左子
unsafe fn rotate_left<V>(root: &mut *mut MapleNode<V>, x: *mut MapleNode<V>) {
    let y = (*x).right;
    (*x).right = (*y).left;
    if !is_null((*y).left) {
        (*(*y).left).parent = x;
    }
    (*y).parent = (*x).parent;
    if is_null((*x).parent) {
        *root = y;
    } else if x == (*(*x).parent).left {
        (*(*x).parent).left = y;
    } else {
        (*(*x).parent).right = y;
    }
    (*y).left = x;
    (*x).parent = y;
    update_augmented(x);
    update_augmented(y);
}

/// 右旋：x 的左子 y 上升，x 下降为 y 的右子
unsafe fn rotate_right<V>(root: &mut *mut MapleNode<V>, x: *mut MapleNode<V>) {
    let y = (*x).left;
    (*x).left = (*y).right;
    if !is_null((*y).right) {
        (*(*y).right).parent = x;
    }
    (*y).parent = (*x).parent;
    if is_null((*x).parent) {
        *root = y;
    } else if x == (*(*x).parent).right {
        (*(*x).parent).right = y;
    } else {
        (*(*x).parent).left = y;
    }
    (*y).right = x;
    (*x).parent = y;
    update_augmented(x);
    update_augmented(y);
}

/// 用子树 v 替换子树 u 在其父节点中的位置
unsafe fn transplant<V>(root: &mut *mut MapleNode<V>, u: *mut MapleNode<V>, v: *mut MapleNode<V>) {
    if is_null((*u).parent) {
        *root = v;
    } else if u == (*(*u).parent).left {
        (*(*u).parent).left = v;
    } else {
        (*(*u).parent).right = v;
    }
    if !is_null(v) {
        (*v).parent = (*u).parent;
    }
}

/// 插入后修复红黑性质
unsafe fn rb_insert_fixup<V>(root: &mut *mut MapleNode<V>, mut z: *mut MapleNode<V>) {
    while !is_null(parent(z)) && color(parent(z)) == Color::Red {
        let p = parent(z);
        let gp = parent(p);
        if p == left(gp) {
            let uncle = right(gp);
            if color(uncle) == Color::Red {
                // Case 1: 叔节点为红
                set_color(p, Color::Black);
                set_color(uncle, Color::Black);
                set_color(gp, Color::Red);
                z = gp;
            } else {
                if z == right(p) {
                    // Case 2: z 是右子 → 左旋转化为 Case 3
                    z = p;
                    rotate_left(root, z);
                }
                // Case 3: z 是左子
                let p = parent(z);
                let gp = parent(p);
                set_color(p, Color::Black);
                set_color(gp, Color::Red);
                rotate_right(root, gp);
            }
        } else {
            // 镜像情况
            let uncle = left(gp);
            if color(uncle) == Color::Red {
                set_color(p, Color::Black);
                set_color(uncle, Color::Black);
                set_color(gp, Color::Red);
                z = gp;
            } else {
                if z == left(p) {
                    z = p;
                    rotate_right(root, z);
                }
                let p = parent(z);
                let gp = parent(p);
                set_color(p, Color::Black);
                set_color(gp, Color::Red);
                rotate_left(root, gp);
            }
        }
    }
    set_color(*root, Color::Black);
}

/// 删除后修复红黑性质
unsafe fn rb_delete_fixup<V>(root: &mut *mut MapleNode<V>, mut x: *mut MapleNode<V>, mut x_parent: *mut MapleNode<V>) {
    while x != *root && color(x) == Color::Black {
        let p = if !is_null(x) { parent(x) } else { x_parent };
        if is_null(p) {
            break;
        }
        if x == left(p) {
            let mut w = right(p);
            if color(w) == Color::Red {
                // Case 1
                set_color(w, Color::Black);
                set_color(p, Color::Red);
                rotate_left(root, p);
                w = right(p);
            }
            if color(left(w)) == Color::Black && color(right(w)) == Color::Black {
                // Case 2
                set_color(w, Color::Red);
                x = p;
                x_parent = parent(p);
            } else {
                if color(right(w)) == Color::Black {
                    // Case 3
                    set_color(left(w), Color::Black);
                    set_color(w, Color::Red);
                    rotate_right(root, w);
                    w = right(p);
                }
                // Case 4
                set_color(w, color(p));
                set_color(p, Color::Black);
                set_color(right(w), Color::Black);
                rotate_left(root, p);
                x = *root;
                x_parent = ptr::null_mut();
            }
        } else {
            // 镜像
            let mut w = left(p);
            if color(w) == Color::Red {
                set_color(w, Color::Black);
                set_color(p, Color::Red);
                rotate_right(root, p);
                w = left(p);
            }
            if color(right(w)) == Color::Black && color(left(w)) == Color::Black {
                set_color(w, Color::Red);
                x = p;
                x_parent = parent(p);
            } else {
                if color(left(w)) == Color::Black {
                    set_color(right(w), Color::Black);
                    set_color(w, Color::Red);
                    rotate_left(root, w);
                    w = left(p);
                }
                set_color(w, color(p));
                set_color(p, Color::Black);
                set_color(left(w), Color::Black);
                rotate_right(root, p);
                x = *root;
                x_parent = ptr::null_mut();
            }
        }
    }
    set_color(x, Color::Black);
}

// ---------------------------------------------------------------------------
// MapleTree 实现
// ---------------------------------------------------------------------------

impl<V> MapleTree<V> {
    /// 创建空树
    pub const fn new() -> Self {
        Self {
            root: ptr::null_mut(),
            len: 0,
        }
    }

    /// 元素数量
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// 是否为空
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    // -----------------------------------------------------------------------
    // 内部查找辅助
    // -----------------------------------------------------------------------

    /// 查找 start 最大且 <= addr 的节点（floor by start）
    unsafe fn floor_node(&self, addr: usize) -> *mut MapleNode<V> {
        let mut result: *mut MapleNode<V> = ptr::null_mut();
        let mut cur = self.root;
        while !is_null(cur) {
            if (*cur).start <= addr {
                result = cur;
                cur = (*cur).right;
            } else {
                cur = (*cur).left;
            }
        }
        result
    }

    /// 查找 start 最小且 >= start 的节点（lower_bound by start）
    unsafe fn lower_bound_node(&self, start: usize) -> *mut MapleNode<V> {
        let mut result: *mut MapleNode<V> = ptr::null_mut();
        let mut cur = self.root;
        while !is_null(cur) {
            if (*cur).start >= start {
                result = cur;
                cur = (*cur).left;
            } else {
                cur = (*cur).right;
            }
        }
        result
    }

    /// 查找 start 最小且 > start 的节点（upper_bound by start）
    unsafe fn upper_bound_node(&self, start: usize) -> *mut MapleNode<V> {
        let mut result: *mut MapleNode<V> = ptr::null_mut();
        let mut cur = self.root;
        while !is_null(cur) {
            if (*cur).start > start {
                result = cur;
                cur = (*cur).left;
            } else {
                cur = (*cur).right;
            }
        }
        result
    }

    /// 查找 start 最大且 < start 的节点
    unsafe fn lower_than_node(&self, start: usize) -> *mut MapleNode<V> {
        let mut result: *mut MapleNode<V> = ptr::null_mut();
        let mut cur = self.root;
        while !is_null(cur) {
            if (*cur).start < start {
                result = cur;
                cur = (*cur).right;
            } else {
                cur = (*cur).left;
            }
        }
        result
    }

    /// 精确查找 start 的节点
    unsafe fn find_exact_node(&self, start: usize) -> *mut MapleNode<V> {
        let mut cur = self.root;
        while !is_null(cur) {
            if start < (*cur).start {
                cur = (*cur).left;
            } else if start > (*cur).start {
                cur = (*cur).right;
            } else {
                return cur;
            }
        }
        ptr::null_mut()
    }

    /// 检查 [start, end) 是否与已有区间重叠
    unsafe fn has_overlap(&self, start: usize, end: usize) -> bool {
        // 查找 start <= addr 的最大节点，检查其 end > start
        let floor = self.floor_node(start);
        if !is_null(floor) && (*floor).end > start {
            return true;
        }
        // 查找 start >= start 的最小节点，检查其 start < end
        let lb = self.lower_bound_node(start);
        if !is_null(lb) && (*lb).start < end {
            return true;
        }
        false
    }

    // -----------------------------------------------------------------------
    // 内部插入
    // -----------------------------------------------------------------------

    /// BST 插入 + 增强字段维护 + RB 修复
    unsafe fn internal_insert(&mut self, node: *mut MapleNode<V>) {
        let mut parent_ptr: *mut MapleNode<V> = ptr::null_mut();
        let mut cur = self.root;
        while !is_null(cur) {
            parent_ptr = cur;
            if (*node).start < (*cur).start {
                cur = (*cur).left;
            } else {
                cur = (*cur).right;
            }
        }
        (*node).parent = parent_ptr;
        (*node).left = ptr::null_mut();
        (*node).right = ptr::null_mut();
        (*node).color = Color::Red;

        if is_null(parent_ptr) {
            self.root = node;
        } else if (*node).start < (*parent_ptr).start {
            (*parent_ptr).left = node;
        } else {
            (*parent_ptr).right = node;
        }

        // 计算 gap_before 和 gap_start：本节点 start 与前驱 end 之间的间隙
        let pred = predecessor(node);
        if is_null(pred) {
            (*node).gap_before = (*node).start;
            (*node).gap_start = 0;
        } else {
            (*node).gap_before = (*node).start.saturating_sub((*pred).end);
            (*node).gap_start = (*pred).end;
        }

        // 更新后继的 gap_before 和 gap_start
        let succ = successor(node);
        if !is_null(succ) {
            (*succ).gap_before = (*succ).start.saturating_sub((*node).end);
            (*succ).gap_start = (*node).end;
            propagate_augmented(succ);
        }

        // 从新节点向上传播增强字段
        propagate_augmented(node);

        self.len += 1;
        rb_insert_fixup(&mut self.root, node);
        // 旋转可能改变增强字段，再次从 node 向上传播
        propagate_augmented(node);
    }

    // -----------------------------------------------------------------------
    // 内部删除
    // -----------------------------------------------------------------------

    /// 从树中删除节点，返回 Box 以便调用者取出值
    unsafe fn internal_delete(&mut self, z: *mut MapleNode<V>) -> Box<MapleNode<V>> {
        // 更新后继的 gap_before 和 gap_start（后继将继承 z 的前驱关系）
        let z_succ = successor(z);
        let z_pred = predecessor(z);
        if !is_null(z_succ) {
            if is_null(z_pred) {
                (*z_succ).gap_before = (*z_succ).start;
                (*z_succ).gap_start = 0;
            } else {
                (*z_succ).gap_before = (*z_succ).start.saturating_sub((*z_pred).end);
                (*z_succ).gap_start = (*z_pred).end;
            }
        }

        let need_fixup_color: Color;
        let x: *mut MapleNode<V>;
        let x_parent: *mut MapleNode<V>;

        if is_null((*z).left) {
            need_fixup_color = (*z).color;
            x = (*z).right;
            x_parent = (*z).parent;
            transplant(&mut self.root, z, (*z).right);
            if !is_null(x_parent) {
                propagate_augmented(x_parent);
            }
        } else if is_null((*z).right) {
            need_fixup_color = (*z).color;
            x = (*z).left;
            x_parent = (*z).parent;
            transplant(&mut self.root, z, (*z).left);
            if !is_null(x_parent) {
                propagate_augmented(x_parent);
            }
        } else {
            // z 有两个子节点：用后继 y 替换 z
            let y = tree_minimum((*z).right);
            need_fixup_color = (*y).color;
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

            // 从替换位置向上传播增强字段
            propagate_augmented(if is_null(x) { x_parent } else { parent(x) });
            propagate_augmented(y);
        }

        self.len -= 1;

        if need_fixup_color == Color::Black && !is_null(self.root) {
            rb_delete_fixup(&mut self.root, x, x_parent);
            propagate_augmented(if is_null(x) { x_parent } else { x });
        }

        // 断开 z 的指针以避免悬挂
        (*z).left = ptr::null_mut();
        (*z).right = ptr::null_mut();
        (*z).parent = ptr::null_mut();
        Box::from_raw(z)
    }

    // -----------------------------------------------------------------------
    // 公开 API — 查询
    // -----------------------------------------------------------------------

    /// 检查地址是否落在某个区间内
    pub fn contains(&self, addr: usize) -> bool {
        self.find(addr).is_some()
    }

    /// 检查是否存在以 start 为起始的区间
    pub fn contains_start(&self, start: usize) -> bool {
        unsafe { !is_null(self.find_exact_node(start)) }
    }

    /// 查找包含 addr 的区间 [start, end)
    pub fn find(&self, addr: usize) -> Option<(usize, usize, &V)> {
        unsafe {
            let node = self.floor_node(addr);
            if !is_null(node) && addr < (*node).end {
                Some(((*node).start, (*node).end, &(*node).value))
            } else {
                None
            }
        }
    }

    /// 查找包含 addr 的区间（可变引用）
    pub fn find_mut(&mut self, addr: usize) -> Option<(usize, usize, &mut V)> {
        unsafe {
            let node = self.floor_node(addr);
            if !is_null(node) && addr < (*node).end {
                Some(((*node).start, (*node).end, &mut (*node).value))
            } else {
                None
            }
        }
    }

    /// 按 start 精确查找
    pub fn find_by_start(&self, start: usize) -> Option<(usize, &V)> {
        unsafe {
            let node = self.find_exact_node(start);
            if is_null(node) {
                None
            } else {
                Some(((*node).end, &(*node).value))
            }
        }
    }

    /// 按 start 精确查找（可变引用）
    pub fn find_by_start_mut(&mut self, start: usize) -> Option<(usize, &mut V)> {
        unsafe {
            let node = self.find_exact_node(start);
            if is_null(node) {
                None
            } else {
                Some(((*node).end, &mut (*node).value))
            }
        }
    }

    /// 最小区间
    pub fn first(&self) -> Option<(usize, usize, &V)> {
        unsafe {
            let node = tree_minimum(self.root);
            if is_null(node) {
                None
            } else {
                Some(((*node).start, (*node).end, &(*node).value))
            }
        }
    }

    /// 最大区间
    pub fn last(&self) -> Option<(usize, usize, &V)> {
        unsafe {
            let node = tree_maximum(self.root);
            if is_null(node) {
                None
            } else {
                Some(((*node).start, (*node).end, &(*node).value))
            }
        }
    }

    /// 返回第一个 start >= given 的区间
    pub fn lower_bound(&self, start: usize) -> Option<(usize, usize, &V)> {
        unsafe {
            let node = self.lower_bound_node(start);
            if is_null(node) {
                None
            } else {
                Some(((*node).start, (*node).end, &(*node).value))
            }
        }
    }

    /// 返回第一个 start > given 的区间
    pub fn upper_bound(&self, start: usize) -> Option<(usize, usize, &V)> {
        unsafe {
            let node = self.upper_bound_node(start);
            if is_null(node) {
                None
            } else {
                Some(((*node).start, (*node).end, &(*node).value))
            }
        }
    }

    /// 返回最后一个 start <= given 的区间
    pub fn floor(&self, start: usize) -> Option<(usize, usize, &V)> {
        unsafe {
            let node = self.floor_node(start);
            if is_null(node) {
                None
            } else {
                Some(((*node).start, (*node).end, &(*node).value))
            }
        }
    }

    /// 返回最后一个 start < given 的区间
    pub fn lower_than(&self, start: usize) -> Option<(usize, usize, &V)> {
        unsafe {
            let node = self.lower_than_node(start);
            if is_null(node) {
                None
            } else {
                Some(((*node).start, (*node).end, &(*node).value))
            }
        }
    }

    // -----------------------------------------------------------------------
    // 公开 API — 插入 / 删除
    // -----------------------------------------------------------------------

    /// 插入区间 [start, end)，不允许重叠
    pub fn insert(&mut self, start: usize, end: usize, value: V) -> Result<(), MapleInsertError> {
        if start >= end {
            return Err(MapleInsertError::InvalidRange);
        }
        unsafe {
            if self.has_overlap(start, end) {
                return Err(MapleInsertError::Overlap);
            }
            let node = MapleNode::new(start, end, value);
            self.internal_insert(node);
        }
        Ok(())
    }

    /// 就地修改以 start 为起始的区间的终点
    ///
    /// 若新终点与后继区间重叠则返回 Overlap 错误，不做任何修改。
    /// 返回旧的 end 值。
    pub fn update_end(&mut self, start: usize, new_end: usize) -> Result<usize, MapleInsertError> {
        if start >= new_end {
            return Err(MapleInsertError::InvalidRange);
        }
        unsafe {
            let node = self.find_exact_node(start);
            if is_null(node) {
                return Err(MapleInsertError::InvalidRange);
            }
            let old_end = (*node).end;
            if new_end == old_end {
                return Ok(old_end);
            }
            // 检查新 end 是否与后继区间重叠
            if new_end > old_end {
                let succ = successor(node);
                if !is_null(succ) && new_end > (*succ).start {
                    return Err(MapleInsertError::Overlap);
                }
            }
            // 更新 end
            (*node).end = new_end;
            // 更新后继的 gap_before 和 gap_start
            let succ = successor(node);
            if !is_null(succ) {
                (*succ).gap_before = (*succ).start.saturating_sub(new_end);
                (*succ).gap_start = new_end;
                propagate_augmented(succ);
            }
            // 从当前节点向上传播增强字段（max_end 可能变化）
            propagate_augmented(node);
            Ok(old_end)
        }
    }

    /// 按 start 精确删除，返回 (end, value)
    pub fn remove(&mut self, start: usize) -> Option<(usize, V)> {
        unsafe {
            let node = self.find_exact_node(start);
            if is_null(node) {
                return None;
            }
            let boxed = self.internal_delete(node);
            Some((boxed.end, boxed.value))
        }
    }

    /// 删除包含 addr 的区间，返回 (start, end, value)
    pub fn remove_at(&mut self, addr: usize) -> Option<(usize, usize, V)> {
        unsafe {
            let node = self.floor_node(addr);
            if is_null(node) || addr >= (*node).end {
                return None;
            }
            let s = (*node).start;
            let e = (*node).end;
            let boxed = self.internal_delete(node);
            Some((s, e, boxed.value))
        }
    }

    /// 删除所有与 [start, end) 相交的区间
    pub fn remove_intersecting(&mut self, start: usize, end: usize) -> Vec<(usize, usize, V)> {
        let mut result = Vec::new();
        if start >= end {
            return result;
        }
        // 收集所有相交区间的 start 值
        let mut to_remove = Vec::new();
        unsafe {
            // 从 floor(start) 开始检查
            let mut node = self.floor_node(start);
            if is_null(node) {
                node = tree_minimum(self.root);
            }
            while !is_null(node) {
                if (*node).start >= end {
                    break;
                }
                // 区间 [ns, ne) 与 [start, end) 相交 iff ns < end && ne > start
                if (*node).end > start && (*node).start < end {
                    to_remove.push((*node).start);
                }
                node = successor(node);
            }
        }
        for s in to_remove {
            if let Some((e, v)) = self.remove(s) {
                result.push((s, e, v));
            }
        }
        result
    }

    /// 弹出最小区间
    pub fn pop_first(&mut self) -> Option<(usize, usize, V)> {
        unsafe {
            let node = tree_minimum(self.root);
            if is_null(node) {
                return None;
            }
            let s = (*node).start;
            let e = (*node).end;
            let boxed = self.internal_delete(node);
            Some((s, e, boxed.value))
        }
    }

    /// 弹出最大区间
    pub fn pop_last(&mut self) -> Option<(usize, usize, V)> {
        unsafe {
            let node = tree_maximum(self.root);
            if is_null(node) {
                return None;
            }
            let s = (*node).start;
            let e = (*node).end;
            let boxed = self.internal_delete(node);
            Some((s, e, boxed.value))
        }
    }

    /// 清空所有区间
    pub fn clear(&mut self) {
        unsafe {
            free_subtree(self.root);
        }
        self.root = ptr::null_mut();
        self.len = 0;
    }

    /// 将 other 中的所有区间移入 self，返回因重叠而无法插入的区间
    pub fn append(&mut self, other: &mut Self) -> Vec<(usize, usize, V)> {
        let mut failed = Vec::new();
        while let Some((s, e, v)) = other.pop_first() {
            unsafe {
                if self.has_overlap(s, e) {
                    failed.push((s, e, v));
                } else {
                    let node = MapleNode::new(s, e, v);
                    self.internal_insert(node);
                }
            }
        }
        failed
    }

    /// 保留满足条件的区间，删除不满足的
    pub fn retain<F: FnMut(usize, usize, &mut V) -> bool>(&mut self, mut f: F) {
        let mut to_remove = Vec::new();
        unsafe {
            let mut node = tree_minimum(self.root);
            while !is_null(node) {
                if !f((*node).start, (*node).end, &mut (*node).value) {
                    to_remove.push((*node).start);
                }
                node = successor(node);
            }
        }
        for start in to_remove {
            self.remove(start);
        }
    }

    /// 插入并覆盖：先删除所有与 [start, end) 相交的区间，再插入
    pub fn insert_overwrite(
        &mut self,
        start: usize,
        end: usize,
        value: V,
    ) -> Result<Vec<(usize, usize, V)>, MapleInsertError> {
        if start >= end {
            return Err(MapleInsertError::InvalidRange);
        }
        let removed = self.remove_intersecting(start, end);
        // 插入不应失败，因为已清除重叠
        self.insert(start, end, value)
            .map_err(|_| MapleInsertError::Overlap)?;
        Ok(removed)
    }

    /// 替换：若存在以 start 为起始的区间，替换其 end 和 value
    pub fn replace(
        &mut self,
        start: usize,
        end: usize,
        value: V,
    ) -> Result<Option<(usize, V)>, MapleInsertError> {
        if start >= end {
            return Err(MapleInsertError::InvalidRange);
        }
        // 先尝试删除旧的
        let old = self.remove(start);
        // 检查新范围是否与其他区间重叠
        unsafe {
            if self.has_overlap(start, end) {
                // 恢复旧区间
                if let Some((old_end, old_val)) = old {
                    let node = MapleNode::new(start, old_end, old_val);
                    self.internal_insert(node);
                }
                return Err(MapleInsertError::Overlap);
            }
        }
        let node = MapleNode::new(start, end, value);
        unsafe {
            self.internal_insert(node);
        }
        Ok(old)
    }

    // -----------------------------------------------------------------------
    // 间隙搜索
    // -----------------------------------------------------------------------

    /// 在 [hint_start, limit) 范围内查找大小 >= min_size 的间隙
    ///
    /// 利用 max_gap 增强字段实现 O(log n) 剪枝搜索。
    pub fn find_gap(&self, min_size: usize, hint_start: usize, limit: usize) -> Option<usize> {
        if min_size == 0 {
            return Some(hint_start);
        }
        if hint_start.checked_add(min_size)? > limit {
            return None;
        }
        unsafe {
            if is_null(self.root) {
                // 空树，整个范围都是间隙
                return Some(hint_start);
            }

            // 递归搜索子树（包含首节点前间隙的检查）
            if let Some(addr) = self.find_gap_in_subtree(self.root, min_size, hint_start, limit) {
                return Some(addr);
            }

            // 检查最后一个节点之后的间隙
            let last = tree_maximum(self.root);
            if !is_null(last) {
                let gap_start = if (*last).end > hint_start {
                    (*last).end
                } else {
                    hint_start
                };
                if gap_start.checked_add(min_size)? <= limit {
                    return Some(gap_start);
                }
            }

            None
        }
    }

    /// 在子树中搜索间隙（利用 max_gap 剪枝）
    unsafe fn find_gap_in_subtree(
        &self,
        node: *mut MapleNode<V>,
        min_size: usize,
        after: usize,
        limit: usize,
    ) -> Option<usize> {
        if is_null(node) {
            return None;
        }
        if (*node).max_gap < min_size {
            return None; // 剪枝：子树中没有足够大的间隙
        }

        // 先搜索左子树
        if !is_null((*node).left) {
            if let Some(addr) = self.find_gap_in_subtree((*node).left, min_size, after, limit) {
                return Some(addr);
            }
        }

        // 检查本节点的 gap_before（使用缓存的 gap_start 避免 O(log n) 的 predecessor 调用）
        let gap_start_raw = (*node).gap_start;
        let effective_start = if gap_start_raw > after {
            gap_start_raw
        } else {
            after
        };
        let gap_end = (*node).start;
        if gap_end > effective_start
            && gap_end - effective_start >= min_size
            && effective_start <= limit
            && limit - effective_start >= min_size
        {
            return Some(effective_start);
        }

        // 搜索右子树
        if !is_null((*node).right) {
            if let Some(addr) = self.find_gap_in_subtree((*node).right, min_size, after, limit) {
                return Some(addr);
            }
        }

        None
    }

    /// 从高地址向低地址搜索大小 >= min_size 的间隙
    ///
    /// 返回间隙的起始地址，使得 [返回值, 返回值 + min_size) 在 [lower_limit, hint_end) 内
    /// 且不与任何已有区间重叠。优先返回最高地址的间隙。
    pub fn find_gap_reverse(
        &self,
        min_size: usize,
        hint_end: usize,
        lower_limit: usize,
    ) -> Option<usize> {
        if min_size == 0 {
            return Some(hint_end.checked_sub(min_size)?);
        }
        if hint_end < lower_limit.checked_add(min_size)? {
            return None;
        }
        unsafe {
            if is_null(self.root) {
                return Some(hint_end.checked_sub(min_size)?);
            }

            // 检查最后一个节点之后的间隙
            let last = tree_maximum(self.root);
            if !is_null(last) {
                let gap_start = (*last).end;
                if gap_start < hint_end && hint_end - gap_start >= min_size && gap_start >= lower_limit {
                    return Some(hint_end - min_size);
                }
            }

            // 从最大节点向前遍历，检查每个节点前的间隙
            let mut node = tree_maximum(self.root);
            while !is_null(node) {
                let gap_end = (*node).start;
                let gap_start_raw = (*node).gap_start;

                if gap_end > gap_start_raw {
                    let effective_end = if gap_end < hint_end { gap_end } else { hint_end };
                    let effective_start = if gap_start_raw > lower_limit {
                        gap_start_raw
                    } else {
                        lower_limit
                    };
                    if effective_end > effective_start
                        && effective_end - effective_start >= min_size
                    {
                        return Some(effective_end - min_size);
                    }
                }
                node = predecessor(node);
            }

            None
        }
    }

    // -----------------------------------------------------------------------
    // 迭代器
    // -----------------------------------------------------------------------

    /// 按 start 升序迭代所有区间
    pub fn iter(&self) -> impl Iterator<Item = (usize, usize, &V)> {
        MapleIter {
            current: unsafe { tree_minimum(self.root) },
            _marker: PhantomData,
        }
    }

    /// 按 start 升序迭代所有区间（可变引用）
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (usize, usize, &mut V)> {
        MapleIterMut {
            current: unsafe { tree_minimum(self.root) },
            _marker: PhantomData,
        }
    }

    /// 范围迭代：返回 start 在给定范围内的区间
    pub fn range<R: RangeBounds<usize>>(&self, range: R) -> impl Iterator<Item = (usize, usize, &V)> {
        let lo = match range.start_bound() {
            Bound::Included(&v) => v,
            Bound::Excluded(&v) => v.saturating_add(1),
            Bound::Unbounded => 0,
        };
        let hi = match range.end_bound() {
            Bound::Included(&v) => v.checked_add(1).unwrap_or(usize::MAX),
            Bound::Excluded(&v) => v,
            Bound::Unbounded => usize::MAX,
        };
        let start_node = unsafe { self.lower_bound_node(lo) };
        MapleRangeIter {
            current: start_node,
            end_start: hi,
            _marker: PhantomData,
        }
    }

    /// 范围迭代（可变引用）
    pub fn range_mut<R: RangeBounds<usize>>(
        &mut self,
        range: R,
    ) -> impl Iterator<Item = (usize, usize, &mut V)> {
        let lo = match range.start_bound() {
            Bound::Included(&v) => v,
            Bound::Excluded(&v) => v.saturating_add(1),
            Bound::Unbounded => 0,
        };
        let hi = match range.end_bound() {
            Bound::Included(&v) => v.checked_add(1).unwrap_or(usize::MAX),
            Bound::Excluded(&v) => v,
            Bound::Unbounded => usize::MAX,
        };
        let start_node = unsafe { self.lower_bound_node(lo) };
        MapleRangeIterMut {
            current: start_node,
            end_start: hi,
            _marker: PhantomData,
        }
    }

    /// 迭代所有与 [start, end) 相交的区间
    pub fn iter_intersecting(
        &self,
        start: usize,
        end: usize,
    ) -> impl Iterator<Item = (usize, usize, &V)> {
        // 相交条件：node.start < end && node.end > start
        // 从 floor(start) 开始（它可能 end > start），然后向后遍历直到 node.start >= end
        let first = unsafe {
            let f = self.floor_node(start);
            if !is_null(f) && (*f).end > start {
                f
            } else {
                self.lower_bound_node(start)
            }
        };
        MapleIntersectIter {
            current: first,
            query_start: start,
            query_end: end,
            _marker: PhantomData,
        }
    }

    /// 迭代所有与 [start, end) 相交的区间（可变引用）
    pub fn iter_intersecting_mut(
        &mut self,
        start: usize,
        end: usize,
    ) -> impl Iterator<Item = (usize, usize, &mut V)> {
        let first = unsafe {
            let f = self.floor_node(start);
            if !is_null(f) && (*f).end > start {
                f
            } else {
                self.lower_bound_node(start)
            }
        };
        MapleIntersectIterMut {
            current: first,
            query_start: start,
            query_end: end,
            _marker: PhantomData,
        }
    }
}

// ---------------------------------------------------------------------------
// V: Clone 特化方法
// ---------------------------------------------------------------------------

impl<V: Clone> MapleTree<V> {
    /// 在 addr 处分割包含该地址的区间
    ///
    /// 将 [start, end) 分割为 [start, addr) 和 [addr, end)，两者共享克隆的值。
    /// 若 addr 不在任何区间内部（即 addr == start 或 addr >= end），返回 false。
    pub fn split_at(&mut self, addr: usize) -> bool {
        let (start, end) = match self.find(addr) {
            Some((s, e, _)) => {
                if addr == s || addr >= e {
                    return false;
                }
                (s, e)
            }
            None => return false,
        };
        // 克隆右半部分的值
        let right_value = match self.find_by_start(start) {
            Some((_, v)) => v.clone(),
            None => return false,
        };
        // 删除原区间
        let old_value = match self.remove(start) {
            Some((_, v)) => v,
            None => return false,
        };
        // 插入左半部分
        if self.insert(start, addr, old_value).is_err() {
            return false;
        }
        // 插入右半部分
        if self.insert(addr, end, right_value).is_err() {
            return false;
        }
        true
    }
}

// ---------------------------------------------------------------------------
// V: PartialEq 特化方法
// ---------------------------------------------------------------------------

impl<V: PartialEq> MapleTree<V> {
    /// 合并相邻且值相等的区间
    ///
    /// 若 [a, b) 和 [b, c) 的值相等，合并为 [a, c)。支持链式合并。返回合并次数。
    pub fn merge_adjacent_equal(&mut self) -> usize {
        let mut total = 0usize;
        loop {
            let round = self.merge_one_pass();
            if round == 0 {
                break;
            }
            total += round;
        }
        total
    }

    /// 单遍合并：收集相邻且值相等的对，执行合并，返回本轮合并次数
    fn merge_one_pass(&mut self) -> usize {
        let mut merges = 0usize;
        let mut to_merge = Vec::new();
        unsafe {
            let mut node = tree_minimum(self.root);
            while !is_null(node) {
                let next = successor(node);
                if !is_null(next)
                    && (*node).end == (*next).start
                    && (*node).value == (*next).value
                {
                    to_merge.push(((*node).start, (*next).start, (*next).end));
                    // 跳过 next 以避免同一节点参与两个合并对
                    node = successor(next);
                } else {
                    node = next;
                }
            }
        }
        for (start1, start2, merged_end) in to_merge {
            self.remove(start2);
            if let Some((_, value)) = self.remove(start1) {
                let _ = self.insert(start1, merged_end, value);
                merges += 1;
            }
        }
        merges
    }
}

// ---------------------------------------------------------------------------
// 迭代器类型
// ---------------------------------------------------------------------------

/// 不可变迭代器
struct MapleIter<'a, V> {
    current: *mut MapleNode<V>,
    _marker: PhantomData<&'a V>,
}

impl<'a, V> Iterator for MapleIter<'a, V> {
    type Item = (usize, usize, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        if self.current.is_null() {
            return None;
        }
        unsafe {
            let node = &*self.current;
            let result = (node.start, node.end, &node.value);
            self.current = successor(self.current);
            Some(result)
        }
    }
}

/// 可变迭代器
struct MapleIterMut<'a, V> {
    current: *mut MapleNode<V>,
    _marker: PhantomData<&'a mut V>,
}

impl<'a, V> Iterator for MapleIterMut<'a, V> {
    type Item = (usize, usize, &'a mut V);

    fn next(&mut self) -> Option<Self::Item> {
        if self.current.is_null() {
            return None;
        }
        unsafe {
            let node = &mut *self.current;
            let result = (node.start, node.end, &mut node.value);
            self.current = successor(self.current);
            Some(result)
        }
    }
}

/// 范围迭代器（不可变）
struct MapleRangeIter<'a, V> {
    current: *mut MapleNode<V>,
    end_start: usize,
    _marker: PhantomData<&'a V>,
}

impl<'a, V> Iterator for MapleRangeIter<'a, V> {
    type Item = (usize, usize, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        if self.current.is_null() {
            return None;
        }
        unsafe {
            let node = &*self.current;
            if node.start >= self.end_start {
                return None;
            }
            let result = (node.start, node.end, &node.value);
            self.current = successor(self.current);
            Some(result)
        }
    }
}

/// 范围迭代器（可变）
struct MapleRangeIterMut<'a, V> {
    current: *mut MapleNode<V>,
    end_start: usize,
    _marker: PhantomData<&'a mut V>,
}

impl<'a, V> Iterator for MapleRangeIterMut<'a, V> {
    type Item = (usize, usize, &'a mut V);

    fn next(&mut self) -> Option<Self::Item> {
        if self.current.is_null() {
            return None;
        }
        unsafe {
            let node = &mut *self.current;
            if node.start >= self.end_start {
                return None;
            }
            let result = (node.start, node.end, &mut node.value);
            self.current = successor(self.current);
            Some(result)
        }
    }
}

/// 相交区间迭代器（不可变）
struct MapleIntersectIter<'a, V> {
    current: *mut MapleNode<V>,
    query_start: usize,
    query_end: usize,
    _marker: PhantomData<&'a V>,
}

impl<'a, V> Iterator for MapleIntersectIter<'a, V> {
    type Item = (usize, usize, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        while !self.current.is_null() {
            unsafe {
                let node = &*self.current;
                self.current = successor(self.current);
                // 相交条件：node.start < query_end && node.end > query_start
                if node.start >= self.query_end {
                    return None; // 后续节点 start 更大，不可能再相交
                }
                if node.end > self.query_start {
                    return Some((node.start, node.end, &node.value));
                }
            }
        }
        None
    }
}

/// 相交区间迭代器（可变）
struct MapleIntersectIterMut<'a, V> {
    current: *mut MapleNode<V>,
    query_start: usize,
    query_end: usize,
    _marker: PhantomData<&'a mut V>,
}

impl<'a, V> Iterator for MapleIntersectIterMut<'a, V> {
    type Item = (usize, usize, &'a mut V);

    fn next(&mut self) -> Option<Self::Item> {
        while !self.current.is_null() {
            unsafe {
                let node = &mut *self.current;
                self.current = successor(self.current);
                if node.start >= self.query_end {
                    return None;
                }
                if node.end > self.query_start {
                    return Some((node.start, node.end, &mut node.value));
                }
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Drop / Default
// ---------------------------------------------------------------------------

impl<V> Drop for MapleTree<V> {
    fn drop(&mut self) {
        unsafe {
            free_subtree(self.root);
        }
        self.root = ptr::null_mut();
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
        f.debug_struct("MapleTree")
            .field("len", &self.len)
            .field(
                "intervals",
                &DebugIntervals { tree: self },
            )
            .finish()
    }
}

struct DebugIntervals<'a, V> {
    tree: &'a MapleTree<V>,
}

impl<'a, V: fmt::Debug> fmt::Debug for DebugIntervals<'a, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.tree.iter().map(|(s, e, v)| (s, e, v)))
            .finish()
    }
}
