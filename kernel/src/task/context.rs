//! 通用上下文定义

/// 任务上下文 trait
///
/// 不同的架构需要实现此 trait
pub trait Context {
    /// 创建空上下文
    fn empty() -> Self;
    
    /// 创建新任务上下文
    fn new(entry: usize, sp: usize) -> Self;
}

// 重新导出架构相关的 Context 实现
pub use super::arch::TaskContext;
