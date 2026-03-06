//! 侵入式双向链表 (Intrusive List)
//!
//! 与标准库链表不同，节点嵌入在宿主结构体中，不额外分配内存。

/// 将字段指针还原为宿主结构体指针。
#[macro_export]
macro_rules! container_of {
    ($ptr:expr, $type:ty, $field:ident) => {{
        let __ptr = $ptr as *const u8;
        let __offset = core::mem::offset_of!($type, $field);
        __ptr.wrapping_sub(__offset) as *mut $type
    }};
}

/// 根据链表节点指针获取宿主结构体指针。
#[macro_export]
macro_rules! list_entry {
    ($ptr:expr, $type:ty, $field:ident) => {{
        $crate::container_of!($ptr, $type, $field)
    }};
}

/// 正向遍历链表。
#[macro_export]
macro_rules! list_for_each {
    ($head:expr, $pos:ident, $body:block) => {{
        let __head = $head as *mut $crate::libs::list::ListHead;
        let mut $pos = unsafe { (*__head).next };

        while $pos != __head {
            {
                $body
            }
            $pos = unsafe { (*$pos).next };
        }
    }};
}

/// 安全删除场景的正向遍历（提前保存 next）。
#[macro_export]
macro_rules! list_for_each_safe {
    ($head:expr, $pos:ident, $next:ident, $body:block) => {{
        let __head = $head as *mut $crate::libs::list::ListHead;
        let mut $pos = unsafe { (*__head).next };

        while $pos != __head {
            let $next = unsafe { (*$pos).next };
            {
                $body
            }
            $pos = $next;
        }
    }};
}

/// 反向遍历链表。
#[macro_export]
macro_rules! list_for_each_prev {
    ($head:expr, $pos:ident, $body:block) => {{
        let __head = $head as *mut $crate::libs::list::ListHead;
        let mut $pos = unsafe { (*__head).prev };

        while $pos != __head {
            {
                $body
            }
            $pos = unsafe { (*$pos).prev };
        }
    }};
}

/// 安全删除场景的反向遍历（提前保存 prev）。
#[macro_export]
macro_rules! list_for_each_prev_safe {
    ($head:expr, $pos:ident, $prev:ident, $body:block) => {{
        let __head = $head as *mut $crate::libs::list::ListHead;
        let mut $pos = unsafe { (*__head).prev };

        while $pos != __head {
            let $prev = unsafe { (*$pos).prev };
            {
                $body
            }
            $pos = $prev;
        }
    }};
}

/// 正向遍历容器项（`$pos` 为 `*mut $type`）。
#[macro_export]
macro_rules! list_for_each_entry {
    ($head:expr, $pos:ident, $type:ty, $field:ident, $body:block) => {{
        let __head = $head as *mut $crate::libs::list::ListHead;
        let mut __node = unsafe { (*__head).next };

        while __node != __head {
            let $pos = $crate::list_entry!(__node, $type, $field);
            {
                $body
            }
            __node = unsafe { (*__node).next };
        }
    }};
}

/// 删除场景的正向容器遍历（`$pos` 为 `*mut $type`）。
#[macro_export]
macro_rules! list_for_each_entry_safe {
    ($head:expr, $pos:ident, $next:ident, $type:ty, $field:ident, $body:block) => {{
        let __head = $head as *mut $crate::libs::list::ListHead;
        let mut __node = unsafe { (*__head).next };

        while __node != __head {
            let __next_node = unsafe { (*__node).next };
            let $pos = $crate::list_entry!(__node, $type, $field);
            let $next = if __next_node == __head {
                core::ptr::null_mut()
            } else {
                $crate::list_entry!(__next_node, $type, $field)
            };
            {
                $body
            }
            __node = __next_node;
        }
    }};
}

/// 双向链表节点（也可作为链表头）
#[repr(C)]
pub struct ListHead {
    pub next: *mut ListHead,
    pub prev: *mut ListHead,
}

impl ListHead {
    /// 创建未初始化节点
    pub const fn new() -> Self {
        Self {
            next: core::ptr::null_mut(),
            prev: core::ptr::null_mut(),
        }
    }

    /// 初始化为自环（空链表头）
    #[inline]
    pub fn init(&mut self) {
        let this = self as *mut _;
        self.next = this;
        self.prev = this;
    }

    /// 是否为空链表
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.next == self as *const _ as *mut _
    }

    /// 谨慎空链表检查（`list_empty_careful` 语义）
    ///
    /// 在可能与并发删除竞争的场景中，比 `is_empty` 更稳健：
    /// 仅当 `next == head` 且 `next == prev` 时才认为为空。
    #[inline]
    pub fn is_empty_careful(&self) -> bool {
        let head = self as *const _ as *mut _;
        let next = self.next;
        let prev = self.prev;
        next == head && next == prev
    }

    /// 是否只有一个节点
    #[inline]
    pub fn is_singular(&self) -> bool {
        !self.is_empty() && self.next == self.prev
    }

    /// 节点是否已链接到某个链表中
    #[inline]
    pub fn is_linked(&self) -> bool {
        !self.next.is_null() && !self.prev.is_null()
    }

    /// 获取首节点（若为空返回 `None`）
    #[inline]
    pub fn front(&self) -> Option<*mut ListHead> {
        if self.is_empty() {
            None
        } else {
            Some(self.next)
        }
    }

    /// 获取尾节点（若为空返回 `None`）
    #[inline]
    pub fn back(&self) -> Option<*mut ListHead> {
        if self.is_empty() {
            None
        } else {
            Some(self.prev)
        }
    }

    /// 将 `new` 插入到 `self` 之后
    ///
    /// # Safety
    /// - `self` 必须是已初始化链表头或已在链表中的节点
    /// - `new` 不能已链接到其他链表
    pub unsafe fn add(&mut self, new: *mut ListHead) {
        unsafe {
            let next = self.next;
            (*new).next = next;
            (*new).prev = self;
            (*next).prev = new;
            self.next = new;
        }
    }

    /// 将 `new` 插入到 `self` 之前（常用于尾插）
    ///
    /// # Safety
    /// 同 `add`
    pub unsafe fn add_tail(&mut self, new: *mut ListHead) {
        unsafe {
            let prev = self.prev;
            (*new).next = self;
            (*new).prev = prev;
            (*prev).next = new;
            self.prev = new;
        }
    }

    /// 将当前节点从链表中删除
    ///
    /// # Safety
    /// 当前节点必须已经在链表中
    pub unsafe fn del(&mut self) {
        unsafe {
            let next = self.next;
            let prev = self.prev;
            (*prev).next = next;
            (*next).prev = prev;
            self.next = core::ptr::null_mut();
            self.prev = core::ptr::null_mut();
        }
    }

    /// 删除并重新初始化为独立节点
    ///
    /// # Safety
    /// 当前节点必须已经在链表中
    pub unsafe fn del_init(&mut self) {
        unsafe {
            self.del();
            self.init();
        }
    }

    /// 弹出首节点（若为空返回 null）
    ///
    /// # Safety
    /// `self` 必须是有效链表头
    pub unsafe fn pop_front(&mut self) -> *mut ListHead {
        if self.is_empty() {
            return core::ptr::null_mut();
        }

        unsafe {
            let node = self.next;
            (*node).del();
            node
        }
    }

    /// 弹出尾节点（若为空返回 null）
    ///
    /// # Safety
    /// `self` 必须是有效链表头
    pub unsafe fn pop_back(&mut self) -> *mut ListHead {
        if self.is_empty() {
            return core::ptr::null_mut();
        }

        unsafe {
            let node = self.prev;
            (*node).del();
            node
        }
    }

    /// 将节点移动到链表头部（head 之后）
    ///
    /// # Safety
    /// - `node` 必须已在某个链表中
    /// - `self` 必须是有效链表头
    pub unsafe fn move_to_front(&mut self, node: *mut ListHead) {
        unsafe {
            (*node).del();
            self.add(node);
        }
    }

    /// 将节点移动到链表尾部（head 之前）
    ///
    /// # Safety
    /// - `node` 必须已在某个链表中
    /// - `self` 必须是有效链表头
    pub unsafe fn move_to_tail(&mut self, node: *mut ListHead) {
        unsafe {
            (*node).del();
            self.add_tail(node);
        }
    }

    /// 用 `new` 替换 `old` 在链表中的位置
    ///
    /// # Safety
    /// - `old` 必须已在链表中
    /// - `new` 不能已链接在其他链表
    pub unsafe fn replace(old: *mut ListHead, new: *mut ListHead) {
        unsafe {
            let next = (*old).next;
            let prev = (*old).prev;
            (*new).next = next;
            (*new).prev = prev;
            (*next).prev = new;
            (*prev).next = new;

            (*old).next = core::ptr::null_mut();
            (*old).prev = core::ptr::null_mut();
        }
    }

    /// 将 `list` 中所有节点拼接到 `self` 头部（`self` 之后）
    ///
    /// # Safety
    /// 两个参数都必须是有效链表头
    pub unsafe fn splice(&mut self, list: *mut ListHead) {
        unsafe {
            if (*list).is_empty() {
                return;
            }

            let first = (*list).next;
            let last = (*list).prev;
            let at = self.next;

            (*first).prev = self;
            self.next = first;

            (*last).next = at;
            (*at).prev = last;

            (*list).init();
        }
    }

    /// 将 `list` 中所有节点拼接到 `self` 尾部（`self` 之前）
    ///
    /// # Safety
    /// 两个参数都必须是有效链表头
    pub unsafe fn splice_tail(&mut self, list: *mut ListHead) {
        unsafe {
            if (*list).is_empty() {
                return;
            }

            let first = (*list).next;
            let last = (*list).prev;
            let at = self.prev;

            (*first).prev = at;
            (*at).next = first;

            (*last).next = self;
            self.prev = last;

            (*list).init();
        }
    }

    /// 原始指针正向迭代器
    pub fn iter_raw(&self) -> ListIter {
        ListIter {
            head: self as *const _ as *mut _,
            cur: self.next,
        }
    }

    /// 原始指针反向迭代器
    pub fn iter_raw_rev(&self) -> ListIterRev {
        ListIterRev {
            head: self as *const _ as *mut _,
            cur: self.prev,
        }
    }

    /// 删除安全的正向原始迭代器（提前缓存 next）
    pub fn iter_raw_safe(&self) -> ListIterSafe {
        let head = self as *const _ as *mut _;
        let cur = self.next;
        let next = if cur == head {
            head
        } else {
            unsafe { (*cur).next }
        };

        ListIterSafe { head, cur, next }
    }

    /// 删除安全的反向原始迭代器（提前缓存 prev）
    pub fn iter_raw_rev_safe(&self) -> ListIterRevSafe {
        let head = self as *const _ as *mut _;
        let cur = self.prev;
        let prev = if cur == head {
            head
        } else {
            unsafe { (*cur).prev }
        };

        ListIterRevSafe { head, cur, prev }
    }
}

impl Default for ListHead {
    fn default() -> Self {
        Self::new()
    }
}

/// 正向原始节点迭代器
pub struct ListIter {
    head: *mut ListHead,
    cur: *mut ListHead,
}

impl Iterator for ListIter {
    type Item = *mut ListHead;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cur == self.head {
            return None;
        }

        let node = self.cur;
        unsafe {
            self.cur = (*node).next;
        }
        Some(node)
    }
}

/// 反向原始节点迭代器
pub struct ListIterRev {
    head: *mut ListHead,
    cur: *mut ListHead,
}

impl Iterator for ListIterRev {
    type Item = *mut ListHead;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cur == self.head {
            return None;
        }

        let node = self.cur;
        unsafe {
            self.cur = (*node).prev;
        }
        Some(node)
    }
}

/// 删除安全的正向原始节点迭代器
pub struct ListIterSafe {
    head: *mut ListHead,
    cur: *mut ListHead,
    next: *mut ListHead,
}

impl Iterator for ListIterSafe {
    type Item = *mut ListHead;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cur == self.head {
            return None;
        }

        let node = self.cur;
        self.cur = self.next;
        self.next = if self.cur == self.head {
            self.head
        } else {
            unsafe { (*self.cur).next }
        };

        Some(node)
    }
}

/// 删除安全的反向原始节点迭代器
pub struct ListIterRevSafe {
    head: *mut ListHead,
    cur: *mut ListHead,
    prev: *mut ListHead,
}

impl Iterator for ListIterRevSafe {
    type Item = *mut ListHead;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cur == self.head {
            return None;
        }

        let node = self.cur;
        self.cur = self.prev;
        self.prev = if self.cur == self.head {
            self.head
        } else {
            unsafe { (*self.cur).prev }
        };

        Some(node)
    }
}
