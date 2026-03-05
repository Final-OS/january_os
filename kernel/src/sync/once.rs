//! 一次性初始化原语
//!
//! 保证代码只执行一次，用于懒初始化。

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU8, Ordering};

/// 状态常量
const INCOMPLETE: u8 = 0;
const RUNNING: u8 = 1;
const COMPLETE: u8 = 2;

/// 一次性初始化
///
/// 保证闭包只执行一次，即使多个线程同时调用。
pub struct Once {
    state: AtomicU8,
}

struct RunningGuard<'a> {
    state: &'a AtomicU8,
    completed: bool,
}

impl<'a> RunningGuard<'a> {
    fn new(state: &'a AtomicU8) -> Self {
        Self {
            state,
            completed: false,
        }
    }

    fn complete(&mut self) {
        self.completed = true;
        self.state.store(COMPLETE, Ordering::Release);
    }
}

impl Drop for RunningGuard<'_> {
    fn drop(&mut self) {
        if !self.completed {
            self.state.store(INCOMPLETE, Ordering::Release);
        }
    }
}

impl Once {
    /// 创建新的 Once
    pub const fn new() -> Self {
        Self {
            state: AtomicU8::new(INCOMPLETE),
        }
    }

    /// 执行初始化（保证只执行一次）
    ///
    /// 如果已经初始化，立即返回。
    /// 如果正在初始化，自旋等待完成。
    /// 如果未初始化，执行闭包。
    pub fn call_once<F: FnOnce()>(&self, f: F) {
        let _ = self.call_once_try(|| {
            f();
            Ok::<(), ()>(())
        });
    }

    /// 尝试执行一次初始化。
    ///
    /// - `Ok(())`: 初始化成功（或已由其他 CPU 成功初始化）。
    /// - `Err(e)`: 本次初始化失败，状态会回滚为未完成，可重试。
    pub fn call_once_try<F, E>(&self, f: F) -> Result<(), E>
    where
        F: FnOnce() -> Result<(), E>,
    {
        if self.is_completed() {
            return Ok(());
        }

        let mut init = Some(f);
        loop {
            match self.state.compare_exchange(
                INCOMPLETE,
                RUNNING,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // 我们获得了执行权。
                    // 使用 guard 保证：失败或 panic unwind 时状态回滚到 INCOMPLETE。
                    let mut guard = RunningGuard::new(&self.state);
                    let f = init.take().expect("call_once_try initializer consumed");
                    f()?;
                    guard.complete();
                    return Ok(());
                }
                Err(COMPLETE) => {
                    return Ok(());
                }
                Err(RUNNING) => {
                    while self.state.load(Ordering::Relaxed) == RUNNING {
                        core::hint::spin_loop();
                    }
                    if self.state.load(Ordering::Acquire) == COMPLETE {
                        return Ok(());
                    }
                    // 可能是其他初始化尝试失败后回滚到 INCOMPLETE，继续重试 CAS。
                }
                Err(_) => unreachable!(),
            }
        }
    }

    /// 检查是否已完成初始化
    #[inline]
    pub fn is_completed(&self) -> bool {
        self.state.load(Ordering::Acquire) == COMPLETE
    }
}

impl Default for Once {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for Once {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Once")
            .field("completed", &self.is_completed())
            .finish()
    }
}

// ============================================================================
// OnceCell - 懒初始化单元
// ============================================================================

/// 懒初始化单元
///
/// 只能被初始化一次的容器。
pub struct OnceCell<T> {
    once: Once,
    value: UnsafeCell<Option<T>>,
}

unsafe impl<T: Send + Sync> Sync for OnceCell<T> {}
unsafe impl<T: Send> Send for OnceCell<T> {}

impl<T> OnceCell<T> {
    /// 创建空的 OnceCell
    pub const fn new() -> Self {
        Self {
            once: Once::new(),
            value: UnsafeCell::new(None),
        }
    }

    /// 创建已初始化的 OnceCell
    pub const fn with_value(value: T) -> Self {
        Self {
            once: Once {
                state: AtomicU8::new(COMPLETE),
            },
            value: UnsafeCell::new(Some(value)),
        }
    }

    /// 获取值的引用
    pub fn get(&self) -> Option<&T> {
        if self.once.is_completed() {
            unsafe { (*self.value.get()).as_ref() }
        } else {
            None
        }
    }

    /// 获取值的可变引用
    pub fn get_mut(&mut self) -> Option<&mut T> {
        (*self.value.get_mut()).as_mut()
    }

    /// 设置值（如果未初始化）
    ///
    /// 返回 Ok(()) 如果成功设置，Err(value) 如果已初始化。
    pub fn set(&self, value: T) -> Result<(), T> {
        let mut value = Some(value);

        self.once.call_once(|| {
            let v = value.take().unwrap();
            unsafe {
                *self.value.get() = Some(v);
            }
        });

        match value {
            None => Ok(()),
            Some(v) => Err(v),
        }
    }

    /// 获取或初始化值
    pub fn get_or_init<F: FnOnce() -> T>(&self, f: F) -> &T {
        self.once.call_once(|| unsafe {
            *self.value.get() = Some(f());
        });

        unsafe { (*self.value.get()).as_ref().unwrap() }
    }

    /// 获取或尝试初始化值
    pub fn get_or_try_init<F, E>(&self, f: F) -> Result<&T, E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        self.once.call_once_try(|| {
            let value = f()?;
            unsafe {
                *self.value.get() = Some(value);
            }
            Ok(())
        })?;

        Ok(unsafe {
            (*self.value.get())
                .as_ref()
                .expect("OnceCell completed without value")
        })
    }

    /// 消费并返回内部值
    pub fn into_inner(self) -> Option<T> {
        self.value.into_inner()
    }

    /// 检查是否已初始化
    pub fn is_initialized(&self) -> bool {
        self.once.is_completed()
    }
}

impl<T> Default for OnceCell<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: core::fmt::Debug> core::fmt::Debug for OnceCell<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.get() {
            Some(v) => f.debug_tuple("OnceCell").field(v).finish(),
            None => f.write_str("OnceCell(<uninit>)"),
        }
    }
}

impl<T: Clone> Clone for OnceCell<T> {
    fn clone(&self) -> Self {
        match self.get() {
            Some(v) => Self::with_value(v.clone()),
            None => Self::new(),
        }
    }
}

impl<T: PartialEq> PartialEq for OnceCell<T> {
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get()
    }
}

impl<T: Eq> Eq for OnceCell<T> {}
