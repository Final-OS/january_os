//! 处理器间中断 (IPI) 支持
//!
//! 提供发送 IPI 的通用接口，底层调用架构相关的实现。

use crate::interrupt;

/// IPI 发送模式
#[derive(Clone, Copy, Debug)]
pub enum IpiDeliveryMode {
    Fixed,
    LowestPriority,
    Smi,
    Nmi,
    Init,
    StartUp,
}

impl IpiDeliveryMode {
    /// 转换为架构特定的投递模式值
    fn to_arch(self) -> u32 {
        match self {
            Self::Fixed => interrupt::ICR_DELIVERY_FIXED,
            Self::LowestPriority => interrupt::ICR_DELIVERY_LOWEST,
            Self::Smi => interrupt::ICR_DELIVERY_SMI,
            Self::Nmi => interrupt::ICR_DELIVERY_NMI,
            Self::Init => interrupt::ICR_DELIVERY_INIT,
            Self::StartUp => interrupt::ICR_DELIVERY_STARTUP,
        }
    }
}

/// 发送 IPI 到指定 CPU (APIC ID)
pub fn send_ipi(apic_id: u32, vector: u8, mode: IpiDeliveryMode) {
    interrupt::send_ipi(
        apic_id,
        vector,
        mode.to_arch(),
        interrupt::ICR_SHORTHAND_NONE,
        interrupt::ICR_LEVEL_ASSERT,
        interrupt::ICR_TRIGGER_EDGE,
    );
}

/// 发送 IPI 到所有 CPU (除自己)
pub fn send_ipi_all_excluding_self(vector: u8) {
    interrupt::send_ipi(
        0, // 目标 ID 在 shorthand 模式下被忽略
        vector,
        interrupt::ICR_DELIVERY_FIXED,
        interrupt::ICR_SHORTHAND_ALL_BUT_SELF,
        interrupt::ICR_LEVEL_ASSERT,
        interrupt::ICR_TRIGGER_EDGE,
    );
}

/// 发送 INIT IPI 到指定 CPU
pub fn send_init_ipi(apic_id: u32) {
    interrupt::send_init_ipi(apic_id);
}

/// 发送 SIPI (Start-up IPI) 到指定 CPU
pub fn send_sipi(apic_id: u32, vector: u8) {
    interrupt::send_sipi(apic_id, vector);
}
