/*
 * january_os 内核链接脚本 (x86_64)
 *
 * 链接脚本的作用：
 * 1. 指定各个段（section）在内存中的位置
 * 2. 确保入口点函数位于二进制文件的开头
 * 3. 丢弃不需要的调试信息和动态链接段
 *
 * 内存布局：
 *   0x100000 (1MB) - 内核加载地址
 *     .text        - 可执行代码
 *     .rodata      - 只读数据（字符串常量等）
 *     .data        - 已初始化的可写数据
 *     .bss         - 未初始化的数据（会被清零）
 */

/* 输出格式：64位 ELF */
OUTPUT_FORMAT(elf64-x86-64)

/* 入口点：_start 函数 */
ENTRY(_start)

/* 内核加载地址：1MB
 * 选择这个地址的原因：
 * - 0x00000 - 0x9FFFF: 传统 DOS 可用内存区域，有各种历史遗留用途
 * - 0xA0000 - 0xFFFFF: 视频内存、BIOS ROM 等
 * - 0x100000+: "扩展内存"，可以自由使用
 */
KERNEL_LOAD_ADDR = 0x100000;

SECTIONS
{
    /* 从内核加载地址开始放置段 */
    . = KERNEL_LOAD_ADDR;

    /*
     * .text 段 - 可执行代码
     * 
     * .text.boot 必须放在最前面！
     * 引导程序直接跳转到 KERNEL_LOAD_ADDR，
     * 所以 _start 函数必须位于这个地址。
     */
    .text : {
        *(.text.boot)    /* 启动代码（包含 _start）必须在最前面 */
        *(.text .text.*) /* 其他所有代码 */
    }

    /*
     * .rodata 段 - 只读数据
     * 
     * 包含：
     * - 字符串字面量
     * - 常量数组
     * - 虚函数表（vtable）
     */
    .rodata : {
        *(.rodata .rodata.*)
    }

    /*
     * .data 段 - 已初始化的可写数据
     * 
     * 包含：
     * - 有初始值的全局变量
     * - 有初始值的静态变量
     */
    .data : {
        *(.data .data.*)
    }

    /*
     * .bss 段 - 未初始化的数据
     * 
     * 这个段在文件中不占空间，加载时会被清零。
     * 
     * 包含：
     * - 没有初始值的全局变量
     * - 没有初始值的静态变量
     */
    .bss : {
        __bss_start = .;    /* BSS 段起始地址（内核可能需要手动清零） */
        *(.bss .bss.*)
        *(COMMON)           /* 旧式 C 的"公共"符号 */
        __bss_end = .;      /* BSS 段结束地址 */
    }

    /* 内核结束地址（用于计算内核大小） */
    __kernel_end = .;

    /*
     * 丢弃不需要的段
     * 
     * 这些段在裸机环境中没有用：
     * - .comment: 编译器版本信息
     * - .note*: 各种注释段
     * - .eh_frame*: C++ 异常处理帧（我们不使用 C++ 异常）
     * - .dynsym, .dynstr, .dynamic: 动态链接信息
     * - .gnu.hash, .hash: 符号哈希表
     * - .interp: 动态链接器路径
     * - .rela*: 重定位信息
     * - .plt*, .got*: 过程链接表和全局偏移表
     */
    /DISCARD/ : {
        *(.comment)
        *(.note*)
        *(.eh_frame*)
        *(.dynsym)
        *(.dynstr)
        *(.dynamic)
        *(.gnu.hash)
        *(.hash)
        *(.interp)
        *(.rela*)
        *(.plt*)
        *(.got*)
    }
}
