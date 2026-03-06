import { defineConfig } from "vitepress";

export default defineConfig({
    title: "january_os",
    description: "基于 Rust 的 x86_64 操作系统，目标实现 Linux ABI 兼容",
    lang: "zh-CN",
    base: "/",

    head: [["link", { rel: "icon", href: "/favicon.ico" }]],

    themeConfig: {
        nav: [
            { text: "首页", link: "/" },
            { text: "指南", link: "/guide/overview" },
            { text: "API 参考", link: "/api/overview" },
            { text: "实现详解", link: "/implementation/overview" },
            { text: "开发进度", link: "/progress/overview" },
        ],

        sidebar: {
            "/guide/": [
                {
                    text: "指南",
                    items: [
                        { text: "概述", link: "/guide/overview" },
                        { text: "配置说明", link: "/guide/configuration" },
                        {
                            text: "Skills 与信息流",
                            link: "/guide/skills-info-flow",
                        },
                    ],
                },
            ],

            "/api/": [
                {
                    text: "API 概览",
                    items: [{ text: "模块索引", link: "/api/overview" },
                        { text: "virt", link: "/api/virt/virt" }],
                },
                {
                    text: "内存管理 API",
                    items: [
                        { text: "memblock", link: "/api/mm/memblock" },
                        { text: "buddy", link: "/api/mm/buddy" },
                        { text: "slub", link: "/api/mm/slub" },
                        { text: "vma", link: "/api/mm/vma" },
                        { text: "vmalloc", link: "/api/mm/vmalloc" },
                        { text: "fault", link: "/api/mm/fault" },
                        { text: "pcp", link: "/api/mm/pcp" },
                        { text: "numa", link: "/api/mm/numa" },
                        { text: "iommu", link: "/api/mm/iommu" },
                        { text: "paging", link: "/api/mm/paging" },
                    ],
                },
                {
                    text: "中断 API",
                    items: [
                        { text: "interrupt", link: "/api/interrupt/interrupt" },
                        { text: "gdt", link: "/api/interrupt/gdt" },
                        { text: "idt", link: "/api/interrupt/idt" },
                        { text: "apic", link: "/api/interrupt/apic" },
                        { text: "pit", link: "/api/interrupt/pit" },
                        { text: "handlers", link: "/api/interrupt/handlers" },
                    ],
                },
                {
                    text: "驱动 API",
                    items: [
                        { text: "acpi", link: "/api/drivers/acpi" },
                        { text: "tty", link: "/api/drivers/tty" },
                        { text: "input", link: "/api/drivers/input" },
                    ],
                },
                {
                    text: "同步原语",
                    items: [
                        { text: "SpinLock", link: "/api/sync/spinlock" },
                        { text: "Mutex", link: "/api/sync/mutex" },
                        { text: "RwLock", link: "/api/sync/rwlock" },
                        { text: "Semaphore", link: "/api/sync/semaphore" },
                        { text: "Once", link: "/api/sync/once" },
                        { text: "Barrier", link: "/api/sync/barrier" },
                    ],
                },
                {
                    text: "安全子系统",
                    items: [{ text: "security", link: "/api/security/overview" }],
                },
                {
                    text: "架构相关",
                    items: [{ text: "x86_64", link: "/api/arch/x86_64" }],
                },
            ],

            "/implementation/": [
                {
                    text: "实现详解",
                    items: [
                        { text: "概述", link: "/implementation/overview" },
                        {
                            text: "系统设计与规划",
                            link: "/implementation/architecture-plan",
                        },
                        { text: "引导流程", link: "/implementation/boot" },
                        {
                            text: "内存初始化",
                            link: "/implementation/memory-init",
                        },
                        { text: "GDT/TSS", link: "/implementation/gdt" },
                        { text: "IDT/异常处理", link: "/implementation/idt" },
                        { text: "APIC", link: "/implementation/apic" },
                        { text: "ACPI 解析", link: "/implementation/acpi" },
                        { text: "IOMMU", link: "/implementation/iommu" },
                        { text: "TTY 子系统", link: "/implementation/tty" },
                        {
                            text: "配置生成器",
                            link: "/implementation/cfg-tool",
                        },
                    ],
                },
            ],

            "/progress/": [
                {
                    text: "开发进度",
                    items: [
                        { text: "总体进度", link: "/progress/overview" },
                        { text: "v0.2 实施计划", link: "/progress/v0.2-plan" },
                    ],
                },
            ],
        },

        socialLinks: [
            { icon: "github", link: "https://github.com/Final-OS/january_os" },
        ],

        footer: {
            message: "基于 MIT 许可发布",
            copyright: "Copyright © 2024-present january_os Contributors",
        },

        lastUpdated: {
            text: "最后更新",
            formatOptions: {
                dateStyle: "short",
                timeStyle: "short",
            },
        },

        search: {
            provider: "local",
        },
    },

    markdown: {
        theme: {
            light: "github-light",
            dark: "github-dark",
        },
    },
});
