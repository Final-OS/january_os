use crate::drivers::acpi;
use crate::{arch, debug, error, info, kprintln, warn};
use core::arch::asm;

/// 关机
pub fn shutdown() -> ! {
    info!("Initiating shutdown...");

    // 显示 ACPI 信息
    if let Some((pm1a, pm1b)) = acpi::get_shutdown_info() {
        debug!("  PM1a_CNT: {:#x}, PM1b_CNT: {:#x}", pm1a, pm1b);
    }

    // 方法1: 使用 ACPI FADT 表（正确方式）
    if let Err(e) = acpi::acpi_shutdown() {
        warn!("ACPI shutdown failed: {}", e);
    }

    unsafe {
        // 方法2: QEMU 调试端口
        asm!("out dx, ax", in("dx") 0x604u16, in("ax") 0x2000u16);

        // 方法3: Bochs/老版 QEMU
        asm!("out dx, ax", in("dx") 0xB004u16, in("ax") 0x2000u16);
    }

    error!("Shutdown failed, halting CPU...");
    loop {
        arch::halt();
    }
}

/// 重启
pub fn reboot() -> ! {
    unsafe {
        // 方法 1: ACPI 重启 (QEMU 支持)
        if let Err(e) = acpi::acpi_reset() {
            warn!("ACPI reset failed: {}", e);
        }

        // 通过 I/O 端口 0xCF9 发送重启命令
        asm!("out dx, al", in("dx") 0xCF9u16, in("al") 0x06u8);

        // 方法 2: 8042 键盘控制器重启
        for _ in 0..10 {
            asm!(
                "in al, 0x64",
                "test al, 0x02",
                "jnz 2f",
                "mov al, 0xFE",
                "out 0x64, al",
                "2:",
                out("al") _,
            );
        }

        // 方法 3: Triple Fault (最后手段)
        let null_idt: [u8; 6] = [0; 6];
        asm!(
            "lidt [{}]",
            "int3",
            in(reg) &null_idt,
        );
    }
    loop {
        arch::halt();
    }
}
