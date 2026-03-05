//! 侵入式哈希链表（HList）
//!
//! 参考 Linux hlist 设计：
//! - `HListHead` 仅保存首节点指针
//! - `HListNode` 保存 `next` 和 `pprev`（前驱的 `next` 指针地址）
//! - 删除节点为 O(1)

/// 将 hlist 节点指针还原为宿主结构体指针。
#[macro_export]
macro_rules! hlist_entry {
    ($ptr:expr, $type:ty, $field:ident) => {{ $crate::container_of!($ptr, $type, $field) }};
}

/// 遍历 hlist 节点（`$pos` 为 `*mut HListNode`）。
#[macro_export]
macro_rules! hlist_for_each {
    ($head:expr, $pos:ident, $body:block) => {{
        let __head = $head as *mut $crate::libs::hlist::HListHead;
        let mut $pos = unsafe { (*__head).first };

        while !$pos.is_null() {
            {
                $body
            }
            $pos = unsafe { (*$pos).next };
        }
    }};
}

/// 删除安全的 hlist 节点遍历（提前保存 next）。
#[macro_export]
macro_rules! hlist_for_each_safe {
    ($head:expr, $pos:ident, $next:ident, $body:block) => {{
        let __head = $head as *mut $crate::libs::hlist::HListHead;
        let mut $pos = unsafe { (*__head).first };

        while !$pos.is_null() {
            let $next = unsafe { (*$pos).next };
            {
                $body
            }
            $pos = $next;
        }
    }};
}

/// 遍历 hlist 宿主项（`$pos` 为 `*mut $type`）。
#[macro_export]
macro_rules! hlist_for_each_entry {
    ($head:expr, $pos:ident, $type:ty, $field:ident, $body:block) => {{
        let __head = $head as *mut $crate::libs::hlist::HListHead;
        let mut __node = unsafe { (*__head).first };

        while !__node.is_null() {
            let $pos = $crate::hlist_entry!(__node, $type, $field);
            {
                $body
            }
            __node = unsafe { (*__node).next };
        }
    }};
}

/// 删除安全的 hlist 宿主项遍历。
#[macro_export]
macro_rules! hlist_for_each_entry_safe {
    ($head:expr, $pos:ident, $next:ident, $type:ty, $field:ident, $body:block) => {{
        let __head = $head as *mut $crate::libs::hlist::HListHead;
        let mut __node = unsafe { (*__head).first };

        while !__node.is_null() {
            let __next_node = unsafe { (*__node).next };
            let $pos = $crate::hlist_entry!(__node, $type, $field);
            let $next = if __next_node.is_null() {
                core::ptr::null_mut()
            } else {
                $crate::hlist_entry!(__next_node, $type, $field)
            };
            {
                $body
            }
            __node = __next_node;
        }
    }};
}

/// hlist 头
#[repr(C)]
pub struct HListHead {
    pub first: *mut HListNode,
}

impl HListHead {
    pub const fn new() -> Self {
        Self {
            first: core::ptr::null_mut(),
        }
    }

    #[inline]
    pub fn init(&mut self) {
        self.first = core::ptr::null_mut();
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.first.is_null()
    }

    /// 头插一个节点
    ///
    /// # Safety
    /// - `node` 不能已经在其他链表中
    pub unsafe fn add_head(&mut self, node: *mut HListNode) {
        unsafe {
            let first = self.first;

            (*node).next = first;
            (*node).pprev = core::ptr::addr_of_mut!(self.first);

            if !first.is_null() {
                (*first).pprev = core::ptr::addr_of_mut!((*node).next);
            }

            self.first = node;
        }
    }

    /// 弹出首节点（若为空返回 null）
    ///
    /// # Safety
    /// `self` 必须是有效 head
    pub unsafe fn pop_front(&mut self) -> *mut HListNode {
        let node = self.first;
        if node.is_null() {
            return core::ptr::null_mut();
        }

        unsafe {
            (*node).del();
            node
        }
    }

    /// 把 `old` 的链表移动到 `self`
    ///
    /// # Safety
    /// 两个 head 必须有效
    pub unsafe fn move_list(&mut self, old: &mut HListHead) {
        unsafe {
            self.first = old.first;
            if !self.first.is_null() {
                (*self.first).pprev = core::ptr::addr_of_mut!(self.first);
            }
            old.first = core::ptr::null_mut();
        }
    }

    /// 清空 head（不修改节点的链接状态）
    #[inline]
    pub fn clear(&mut self) {
        self.first = core::ptr::null_mut();
    }
}

impl Default for HListHead {
    fn default() -> Self {
        Self::new()
    }
}

/// hlist 节点
#[repr(C)]
pub struct HListNode {
    pub next: *mut HListNode,
    pub pprev: *mut *mut HListNode,
}

impl HListNode {
    pub const fn new() -> Self {
        Self {
            next: core::ptr::null_mut(),
            pprev: core::ptr::null_mut(),
        }
    }

    #[inline]
    pub fn init(&mut self) {
        self.next = core::ptr::null_mut();
        self.pprev = core::ptr::null_mut();
    }

    #[inline]
    pub fn is_hashed(&self) -> bool {
        !self.pprev.is_null()
    }

    #[inline]
    pub fn is_unhashed(&self) -> bool {
        self.pprev.is_null()
    }

    /// 从当前 hlist 删除
    ///
    /// # Safety
    /// 当前节点必须处于某个 hlist 中
    pub unsafe fn del(&mut self) -> bool {
        unsafe {
            let pprev = self.pprev;
            if pprev.is_null() {
                return false;
            }

            let next = self.next;
            *pprev = next;
            if !next.is_null() {
                (*next).pprev = pprev;
            }

            self.next = core::ptr::null_mut();
            self.pprev = core::ptr::null_mut();
            true
        }
    }

    /// 删除并初始化
    ///
    /// # Safety
    /// 同 `del`
    pub unsafe fn del_init(&mut self) -> bool {
        unsafe {
            let removed = self.del();
            if removed {
                self.init();
            }
            removed
        }
    }

    /// 将 `new` 插入到 `next` 之前
    ///
    /// # Safety
    /// - `next` 必须在某个 hlist 中
    /// - `new` 不能已经链接到别的 hlist
    pub unsafe fn add_before(new: *mut HListNode, next: *mut HListNode) {
        unsafe {
            let pprev = (*next).pprev;
            (*new).next = next;
            (*new).pprev = pprev;
            (*next).pprev = core::ptr::addr_of_mut!((*new).next);
            *pprev = new;
        }
    }

    /// 将 `new` 插入到 `prev` 之后
    ///
    /// # Safety
    /// - `prev` 必须在某个 hlist 中
    /// - `new` 不能已经链接到别的 hlist
    pub unsafe fn add_after(new: *mut HListNode, prev: *mut HListNode) {
        unsafe {
            let next = (*prev).next;

            (*new).next = next;
            (*new).pprev = core::ptr::addr_of_mut!((*prev).next);
            (*prev).next = new;

            if !next.is_null() {
                (*next).pprev = core::ptr::addr_of_mut!((*new).next);
            }
        }
    }
}

impl Default for HListNode {
    fn default() -> Self {
        Self::new()
    }
}
