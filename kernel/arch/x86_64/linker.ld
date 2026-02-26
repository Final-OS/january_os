/*
 * january_os 内核链接脚本 (x86_64)
 *
 * 设计目标：
 * 1) 固定物理加载地址为 1 MiB；
 * 2) 显式控制各段布局，避免 orphan section 带来的不确定性；
 * 3) 导出完整边界符号，便于启动阶段与调试诊断使用。
 */

OUTPUT_FORMAT(elf64-x86-64)
OUTPUT_ARCH(i386:x86-64)
ENTRY(_start)

KERNEL_LOAD_ADDR = 0x00100000; /* 1 MiB */
PAGE_SIZE        = 0x1000;     /* 4 KiB */

PHDRS
{
    text   PT_LOAD FLAGS(5); /* R | X */
    rodata PT_LOAD FLAGS(4); /* R */
    data   PT_LOAD FLAGS(6); /* R | W */
}

SECTIONS
{
    . = KERNEL_LOAD_ADDR;

    __kernel_start = .;
    __kernel_phys_base = .;

    .text : ALIGN(PAGE_SIZE) {
        __text_start = .;
        KEEP(*(.text.boot))
        *(.text .text.*)
        __text_end = .;
    } :text

    . = ALIGN(PAGE_SIZE);
    .rodata : ALIGN(PAGE_SIZE) {
        __rodata_start = .;
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
        *(.rodata1)
        *(.gcc_except_table .gcc_except_table.*)
        __rodata_end = .;
    } :rodata

    . = ALIGN(8);
    .init_array : ALIGN(8) {
        __init_array_start = .;
        KEEP(*(SORT_BY_INIT_PRIORITY(.init_array.*)))
        KEEP(*(.init_array))
        __init_array_end = .;
    } :data

    . = ALIGN(8);
    .fini_array : ALIGN(8) {
        __fini_array_start = .;
        KEEP(*(SORT_BY_INIT_PRIORITY(.fini_array.*)))
        KEEP(*(.fini_array))
        __fini_array_end = .;
    } :data

    . = ALIGN(PAGE_SIZE);
    .data : ALIGN(PAGE_SIZE) {
        __data_start = .;
        *(.data .data.*)
        *(.sdata .sdata.*)
        *(.got .got.*)
        *(.got.plt)
        *(.data.rel.ro .data.rel.ro.*)
        __data_end = .;
    } :data

    /*
     * 文件镜像结束（kernel.bin 体积对应的上界，不包含 NOLOAD 区）。
     * 如需映射到内存完整镜像，请使用 __kernel_mem_size。
     */
    __kernel_file_end = .;
    __kernel_file_size = __kernel_file_end - __kernel_start;

    .bss (NOLOAD) : ALIGN(PAGE_SIZE) {
        __bss_start = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
        *(COMMON)
        __bss_end = .;
    } :data

    . = ALIGN(PAGE_SIZE);
    __kernel_end = .;
    __kernel_mem_end = .;
    __kernel_mem_size = __kernel_mem_end - __kernel_start;
    __text_size = __text_end - __text_start;
    __rodata_size = __rodata_end - __rodata_start;
    __data_size = __data_end - __data_start;
    __bss_size = __bss_end - __bss_start;

    /DISCARD/ : {
        *(.comment)
        *(.note .note.*)
        *(.eh_frame .eh_frame.*)
        *(.interp)
        *(.dynsym .dynstr .dynamic)
        *(.gnu.hash .gnu.version .gnu.version.*)
        *(.plt .plt.*)
    }
}

ASSERT(__kernel_start == KERNEL_LOAD_ADDR, "kernel start address mismatch");
ASSERT((__kernel_start % PAGE_SIZE) == 0, "kernel start must be page-aligned");
ASSERT(__kernel_end >= __kernel_start, "kernel end before kernel start");
ASSERT(__bss_end >= __bss_start, "bss end before bss start");
