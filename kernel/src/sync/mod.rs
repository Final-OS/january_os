//! 同步原语模块
//!
//! 提供内核使用的基础同步原语。
//!
//! ## 同步原语
//!
//! | 原语 | 说明 | 使用场景 |
//! |------|------|----------|
//! | `SpinLock` | 自旋锁 | 短临界区，中断上下文 |
//! | `IrqSpinLock` | 关中断自旋锁 | 同时被中断与普通上下文访问的数据 |
//! | `Mutex` | 互斥锁 | 一般互斥访问 |
//! | `IrqMutex` | 中断安全互斥锁 | 可能在中断中访问的数据 |
//! | `RwLock` | 读写锁 | 读多写少的场景 |
//! | `Once` | 一次性初始化 | 懒初始化 |
//! | `OnceCell` | 懒初始化单元 | 全局单例 |
//! | `Semaphore` | 信号量 | 资源计数 |
//! | `Barrier` | 屏障 | 多线程同步点 |

mod spinlock;
mod mutex;
mod rwlock;
mod once;
mod semaphore;
mod barrier;

// SpinLock
pub use spinlock::{IrqSpinLock, IrqSpinLockGuard, SpinLock, SpinLockGuard};

// Mutex
pub use mutex::{Mutex, MutexGuard, IrqMutex, IrqMutexGuard};

// RwLock
pub use rwlock::{RwLock, RwLockReadGuard, RwLockWriteGuard};

// Once
pub use once::{Once, OnceCell};

// Semaphore
pub use semaphore::{Semaphore, SemaphorePermit, BoundedSemaphore};

// Barrier
pub use barrier::{Barrier, BarrierWaitResult, ResettableBarrier, CountDownLatch};
