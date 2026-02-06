//! 处理器状态管理
//!
//! 管理每个 CPU 核心的任务状态。

use alloc::sync::Arc;
use crate::sync::Mutex;
use super::task::Task;
use super::arch::__switch;
use super::scheduler::SCHEDULER;

/// 每个 CPU 的处理器状态
pub struct Processor {
    /// 当前正在运行的任务
    current: Option<Arc<Mutex<Task>>>,
    
    /// 空闲任务 (Idle Task)
    idle_task: Option<Arc<Mutex<Task>>>,
}

impl Processor {
    pub const fn new() -> Self {
        Self {
            current: None,
            idle_task: None,
        }
    }
    
    /// 获取当前任务
    pub fn current(&self) -> Option<Arc<Mutex<Task>>> {
        self.current.clone()
    }
    
    /// 设置空闲任务
    pub fn set_idle(&mut self, task: Arc<Mutex<Task>>) {
        self.idle_task = Some(task);
    }
    
    /// 获取空闲任务
    pub fn idle_task(&self) -> Option<Arc<Mutex<Task>>> {
        self.idle_task.clone()
    }
    
    /// 切换到下一个任务
    ///
    /// # Safety
    ///
    /// 涉及底层上下文切换，必须小心。
    pub unsafe fn switch_to(&mut self, next: Arc<Mutex<Task>>) {
        let current = self.current.take();
        self.current = Some(next.clone());
        
        let next_ctx_ptr = {
            let next_inner = next.lock();
            &next_inner.context_sp as *const usize
        };
        
        if let Some(prev) = current {
            let mut prev_inner = prev.lock();
            let prev_ctx_ptr = &mut prev_inner.context_sp as *mut usize;
            
            // 释放锁，因为 switch 不会立即返回
            drop(prev_inner); 
            
            // 真正切换
            __switch(prev_ctx_ptr, next_ctx_ptr);
        } else {
            // 如果当前没有任务 (例如启动阶段)，直接加载下一个任务
            // 创建一个临时的变量用于保存当前状态 (会被丢弃)
            let mut unused_sp: usize = 0;
            __switch(&mut unused_sp as *mut usize, next_ctx_ptr);
        }
    }
}

// TODO: Per-CPU 变量
// 目前暂时使用全局单例模拟单核
static PROCESSOR: Mutex<Processor> = Mutex::new(Processor::new());

/// 获取当前任务
pub fn current_task() -> Option<Arc<Mutex<Task>>> {
    PROCESSOR.lock().current()
}

/// 运行调度循环 (永不返回)
pub fn run() -> ! {
    loop {
        super::scheduler::schedule();
        // 简单的忙等待，避免过度消耗 CPU (实际上应该使用 hlt 或等待中断)
        // core::hint::spin_loop();
        // 或者开启中断等待
        // x86_64::instructions::interrupts::enable_and_hlt();
        
        // 暂时只循环，防止退出
        for _ in 0..10000 {
            core::hint::spin_loop();
        }
    }
}
