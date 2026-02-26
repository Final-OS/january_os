/*
 * january_os 内核链接脚本 (x86_64)
 *
 * 内核加载并运行在物理地址 0x100000 (1MB)
 * Bootloader 会建立页表，提供直接映射区访问所有物理内存
 *
 * 后续可以考虑迁移到高半部分内核，但目前保持简单
 */

OUTPUT_FORMAT(elf64-x86-64)
ENTRY(_start)

/* 内核加载地址：1MB */
KERNEL_LOAD_ADDR = 0x100000;

SECTIONS
{
    . = KERNEL_LOAD_ADDR;

    __kernel_start = .;

    .text : ALIGN(4K) {
        *(.text.boot)
        *(.text .text.*)
    }

    .rodata : ALIGN(4K) {
        *(.rodata .rodata.*)
    }

    .data : ALIGN(4K) {
        *(.data .data.*)
    }

    .bss : ALIGN(4K) {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    }

    __kernel_end = .;

    /DISCARD/ : {
        *(.comment)
        *(.note*)
        *(.eh_frame*)
    }
}
