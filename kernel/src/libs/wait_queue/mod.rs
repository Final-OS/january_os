//! 通用等待队列
//!
//! 当前提供不依赖 task 子系统的最小语义：
//! - 入队/出队（按 token）
//! - 唤醒一个/全部
//! - 按谓词选择唤醒

use alloc::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitState {
    Sleeping,
    Woken,
    Interrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitMode {
    Exclusive,
    Shared,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaitEntry {
    pub token: usize,
    pub state: WaitState,
    pub mode: WaitMode,
}

impl WaitEntry {
    pub const fn new(token: usize, mode: WaitMode) -> Self {
        Self {
            token,
            state: WaitState::Sleeping,
            mode,
        }
    }

    #[inline]
    pub fn is_sleeping(&self) -> bool {
        self.state == WaitState::Sleeping
    }
}

pub struct WaitQueue {
    queue: VecDeque<WaitEntry>,
}

impl WaitQueue {
    pub const fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    #[inline]
    pub fn sleeping_count(&self) -> usize {
        self.queue
            .iter()
            .filter(|entry| entry.is_sleeping())
            .count()
    }

    #[inline]
    pub fn contains(&self, token: usize) -> bool {
        self.queue.iter().any(|entry| entry.token == token)
    }

    pub fn enqueue(&mut self, token: usize) {
        self.enqueue_mode(token, WaitMode::Shared);
    }

    pub fn enqueue_mode(&mut self, token: usize, mode: WaitMode) {
        if self.contains(token) {
            return;
        }
        self.queue.push_back(WaitEntry::new(token, mode));
    }

    pub fn dequeue(&mut self, token: usize) -> Option<WaitEntry> {
        let index = self.queue.iter().position(|entry| entry.token == token)?;
        self.queue.remove(index)
    }

    pub fn mark_interrupted(&mut self, token: usize) -> bool {
        let Some(entry) = self.queue.iter_mut().find(|entry| entry.token == token) else {
            return false;
        };
        entry.state = WaitState::Interrupted;
        true
    }

    pub fn wake_one(&mut self) -> Option<WaitEntry> {
        self.wake_one_if(|_| true)
    }

    pub fn wake_one_if<F>(&mut self, mut predicate: F) -> Option<WaitEntry>
    where
        F: FnMut(&WaitEntry) -> bool,
    {
        let index = self
            .queue
            .iter()
            .position(|entry| entry.is_sleeping() && predicate(entry))?;

        let mut entry = self.queue.remove(index)?;
        entry.state = WaitState::Woken;
        Some(entry)
    }

    pub fn wake_all(&mut self) -> usize {
        self.wake_all_if(|_| true)
    }

    pub fn wake_all_if<F>(&mut self, mut predicate: F) -> usize
    where
        F: FnMut(&WaitEntry) -> bool,
    {
        let mut woke = 0;

        for entry in self.queue.iter_mut() {
            if entry.is_sleeping() && predicate(entry) {
                entry.state = WaitState::Woken;
                woke += 1;
            }
        }

        // Keep interrupted entries so callers can observe and dequeue them explicitly.
        self.queue.retain(|entry| entry.state != WaitState::Woken);

        woke
    }

    pub fn drain_woken(&mut self) -> usize {
        let before = self.queue.len();
        self.queue.retain(|entry| entry.state != WaitState::Woken);
        before - self.queue.len()
    }

    pub fn clear(&mut self) {
        self.queue.clear();
    }
}

impl Default for WaitQueue {
    fn default() -> Self {
        Self::new()
    }
}
