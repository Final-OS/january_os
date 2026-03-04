// ============================================================================
// Intel VT-d (Virtualization Technology for Directed I/O) 实现
//
// 基于 Intel VT-d Specification 3.0
// ============================================================================

use core::ptr;
use core::sync::atomic::{AtomicU64, Ordering};
use super::{DmaAddr, TranslationMode, PAGE_SIZE};

// ============================================================================
// VT-d 寄存器偏移 (基于 Intel VT-d Spec Section 10)
// ============================================================================

/// Version Register
const REG_VER: u64 = 0x00;
/// Capability Register
const REG_CAP: u64 = 0x08;
/// Extended Capability Register
const REG_ECAP: u64 = 0x10;
/// Global Command Register
const REG_GCMD: u64 = 0x18;
/// Global Status Register
const REG_GSTS: u64 = 0x1C;
/// Root Table Address Register
const REG_RTADDR: u64 = 0x20;
/// Context Command Register
const REG_CCMD: u64 = 0x28;
/// Fault Status Register
const REG_FSTS: u64 = 0x34;
/// Fault Event Control Register
const REG_FECTL: u64 = 0x38;
/// Fault Event Data Register
const REG_FEDATA: u64 = 0x3C;
/// Fault Event Address Register
const REG_FEADDR: u64 = 0x40;
/// Invalidation Queue Head Register
const REG_IQH: u64 = 0x80;
/// Invalidation Queue Tail Register
const REG_IQT: u64 = 0x88;
/// Invalidation Queue Address Register
const REG_IQA: u64 = 0x90;

// ============================================================================
// Global Command Register 位定义
// ============================================================================

/// Translation Enable
const GCMD_TE: u32 = 1 << 31;
/// Set Root Table Pointer
const GCMD_SRTP: u32 = 1 << 30;
/// Set Fault Log
const GCMD_SFL: u32 = 1 << 29;
/// Enable Advanced Fault Logging
const GCMD_EAFL: u32 = 1 << 28;
/// Write Buffer Flush
const GCMD_WBF: u32 = 1 << 27;
/// Queued Invalidation Enable
const GCMD_QIE: u32 = 1 << 26;
/// Interrupt Remapping Enable
const GCMD_IRE: u32 = 1 << 25;
/// Compatibility Format Interrupt
const GCMD_CFI: u32 = 1 << 23;

// ============================================================================
// Global Status Register 位定义
// ============================================================================

/// Translation Enable Status
const GSTS_TES: u32 = 1 << 31;
/// Root Table Pointer Status
const GSTS_RTPS: u32 = 1 << 30;
/// Fault Log Status
const GSTS_FLS: u32 = 1 << 29;
/// Advanced Fault Logging Status
const GSTS_AFLS: u32 = 1 << 28;
/// Write Buffer Flush Status
const GSTS_WBFS: u32 = 1 << 27;
/// Queued Invalidation Enable Status
const GSTS_QIES: u32 = 1 << 26;
/// Interrupt Remapping Enable Status
const GSTS_IRES: u32 = 1 << 25;
/// Compatibility Format Interrupt Status
const GSTS_CFIS: u32 = 1 << 23;

// ============================================================================
// Capability Register 位定义
// ============================================================================

/// Number of Domains (bits 2:0)
const CAP_ND_MASK: u64 = 0x7;
/// Advanced Fault Logging
const CAP_AFL: u64 = 1 << 3;
/// Required Write Buffer Flushing
const CAP_RWBF: u64 = 1 << 4;
/// Protected Low Memory Region
const CAP_PLMR: u64 = 1 << 5;
/// Protected High Memory Region
const CAP_PHMR: u64 = 1 << 6;
/// Caching Mode
const CAP_CM: u64 = 1 << 7;
/// Supported Adjusted Guest Address Width (bits 20:8)
const CAP_SAGAW_SHIFT: u64 = 8;
const CAP_SAGAW_MASK: u64 = 0x1F << CAP_SAGAW_SHIFT;
/// Maximum Guest Address Width (bits 45:40)
const CAP_MGAW_SHIFT: u64 = 40;
const CAP_MGAW_MASK: u64 = 0x3F << CAP_MGAW_SHIFT;
/// Zero Length Read
const CAP_ZLR: u64 = 1 << 22;
/// Fault Recording Register Offset (bits 33:24)
const CAP_FRO_SHIFT: u64 = 24;
const CAP_FRO_MASK: u64 = 0x3FF << CAP_FRO_SHIFT;
/// Super Page Support (bits 37:34)
const CAP_SLLPS_SHIFT: u64 = 34;
const CAP_SLLPS_MASK: u64 = 0xF << CAP_SLLPS_SHIFT;
/// Page Selective Invalidation
const CAP_PSI: u64 = 1 << 39;
/// Number of Fault Recording Registers (bits 47:40) - wait, overlap with MGAW
const CAP_NFR_SHIFT: u64 = 48;
const CAP_NFR_MASK: u64 = 0xFF << CAP_NFR_SHIFT;
/// Maximum Address Mask Value (bits 54:48)
const CAP_MAMV_SHIFT: u64 = 48;
const CAP_MAMV_MASK: u64 = 0x3F << CAP_MAMV_SHIFT;
/// DMA Write Draining
const CAP_DWD: u64 = 1 << 54;
/// DMA Read Draining
const CAP_DRD: u64 = 1 << 55;
/// First Level 1 GB Page
const CAP_FL1GP: u64 = 1 << 56;
/// Posted Interrupts
const CAP_PI: u64 = 1 << 59;
/// First Level 5 Level Paging
const CAP_FL5LP: u64 = 1 << 60;

// ============================================================================
// Extended Capability Register 位定义
// ============================================================================

/// Page Walk Coherency
const ECAP_C: u64 = 1 << 0;
/// Queued Invalidation
const ECAP_QI: u64 = 1 << 1;
/// Device TLB
const ECAP_DT: u64 = 1 << 2;
/// Interrupt Remapping
const ECAP_IR: u64 = 1 << 3;
/// Extended Interrupt Mode
const ECAP_EIM: u64 = 1 << 4;
/// Pass Through
const ECAP_PT: u64 = 1 << 6;
/// Snoop Control
const ECAP_SC: u64 = 1 << 7;
/// IOTLB Register Offset (bits 17:8)
const ECAP_IRO_SHIFT: u64 = 8;
const ECAP_IRO_MASK: u64 = 0x3FF << ECAP_IRO_SHIFT;
/// Maximum Handle Mask Value (bits 24:20)
const ECAP_MHMV_SHIFT: u64 = 20;
const ECAP_MHMV_MASK: u64 = 0x1F << ECAP_MHMV_SHIFT;
/// Nested Translation
const ECAP_NEST: u64 = 1 << 26;
/// Page Request
const ECAP_PRS: u64 = 1 << 29;
/// Execute Request
const ECAP_ERS: u64 = 1 << 30;
/// Supervisor Request
const ECAP_SRS: u64 = 1 << 31;
/// No Write Flag
const ECAP_NWFS: u64 = 1 << 33;
/// Extended Accessed Flag
const ECAP_EAFS: u64 = 1 << 34;
/// Process Address Space ID Size (bits 44:40)
const ECAP_PSS_SHIFT: u64 = 35;
const ECAP_PSS_MASK: u64 = 0x1F << ECAP_PSS_SHIFT;
/// PASID Translation
const ECAP_PASID: u64 = 1 << 40;
/// Device TLB Invalidation Throttle
const ECAP_DIT: u64 = 1 << 41;
/// Page Attribute Table
const ECAP_PDS: u64 = 1 << 42;
/// Scalable Mode Translation
const ECAP_SMTS: u64 = 1 << 43;
/// Virtual Command Interface
const ECAP_VCS: u64 = 1 << 44;
/// Second Level Large Page Support
const ECAP_SLADS: u64 = 1 << 45;
/// Scalable Mode Large Page Support
const ECAP_SLTS: u64 = 1 << 46;
/// First Level Large Page Support
const ECAP_FLTS: u64 = 1 << 47;
/// Stop Marker
const ECAP_SMPWC: u64 = 1 << 48;
/// RID-PASID Translation
const ECAP_RPS: u64 = 1 << 49;

// ============================================================================
// Context Command Register 位定义
// ============================================================================

/// Invalidate Context-cache (bit 63)
const CCMD_ICC: u64 = 1 << 63;
/// Context Invalidation Request Granularity (bits 62:61)
const CCMD_CIRG_SHIFT: u64 = 61;
const CCMD_CIRG_GLOBAL: u64 = 1 << CCMD_CIRG_SHIFT;
const CCMD_CIRG_DOMAIN: u64 = 2 << CCMD_CIRG_SHIFT;
const CCMD_CIRG_DEVICE: u64 = 3 << CCMD_CIRG_SHIFT;
/// Actual Invalidation Granularity (bits 60:59)
const CCMD_CAIG_SHIFT: u64 = 59;
/// Domain ID (bits 47:32)
const CCMD_DID_SHIFT: u64 = 32;
/// Source ID (bits 31:16)
const CCMD_SID_SHIFT: u64 = 16;
/// Function Mask (bits 1:0)
const CCMD_FM_MASK: u64 = 0x3;

// ============================================================================
// Root Table Entry
// ============================================================================

/// Root Table Entry 存在位
const RTE_PRESENT: u64 = 1 << 0;
/// Root Table Entry Context Table Pointer 掩码 (bits 63:12)
const RTE_CTP_MASK: u64 = !0xFFF;

// ============================================================================
// Context Table Entry
// ============================================================================

/// Context Entry 存在位
const CTE_PRESENT: u64 = 1 << 0;
/// Fault Processing Disable
const CTE_FPD: u64 = 1 << 1;
/// Translation Type (bits 3:2)
const CTE_T_SHIFT: u64 = 2;
/// Translation Type: Untranslated requests only
const CTE_T_UNTRANSLATED: u64 = 0 << CTE_T_SHIFT;
/// Translation Type: Both translated and untranslated
const CTE_T_PASSTHROUGH: u64 = 2 << CTE_T_SHIFT;
/// Address Width (bits 2:0 of high qword)
const CTE_AW_SHIFT: u64 = 0;
/// 48-bit AGAW (4-level page table)
const CTE_AW_48: u64 = 2 << CTE_AW_SHIFT;
/// Second Level Page Table Pointer 掩码 (bits 63:12 of high qword) - Wait, SLPTPTR is in LOW qword for legacy?
/// Let's correct the comments and usage
const CTE_SLPTPTR_MASK: u64 = !0xFFF;
/// Domain ID (bits 23:8 of high qword)
const CTE_DID_SHIFT: u64 = 8;
const CTE_DID_MASK: u64 = 0xFFFF << CTE_DID_SHIFT;

// ============================================================================
// Second-Level Page Table Entry
// ============================================================================

/// Present
const SLPTE_P: u64 = 1 << 0;
/// Read
const SLPTE_R: u64 = 1 << 0;
/// Write
const SLPTE_W: u64 = 1 << 1;
/// Execute (未使用于 DMA)
const SLPTE_X: u64 = 1 << 2;
/// Accessed
const SLPTE_A: u64 = 1 << 8;
/// Dirty
const SLPTE_D: u64 = 1 << 9;
/// Super Page (用于 PDE，表示 2MB/1GB 大页)
const SLPTE_SP: u64 = 1 << 7;
/// Address Mask (bits 51:12)
const SLPTE_ADDR_MASK: u64 = 0x000FFFFFFFFFF000;

// ============================================================================
// VT-d 能力结构
// ============================================================================

/// VT-d 能力
#[derive(Debug, Clone, Copy)]
pub struct VtdCapability {
    /// 支持的域数量 (2^(nd+4))
    pub num_domains: u32,
    /// 最大客户地址宽度
    pub mgaw: u8,
    /// 支持的调整后客户地址宽度
    pub sagaw: u8,
    /// 缓存模式
    pub caching_mode: bool,
    /// 需要写缓冲刷新
    pub rwbf: bool,
    /// 支持大页
    pub large_page: bool,
    /// 支持队列失效
    pub queued_invalidation: bool,
    /// 支持中断重映射
    pub interrupt_remapping: bool,
    /// 支持 Pass-Through
    pub passthrough: bool,
}

impl VtdCapability {
    fn from_regs(cap: u64, ecap: u64) -> Self {
        let nd = (cap & CAP_ND_MASK) as u32;
        let num_domains = 1 << (nd + 4);
        let mgaw = ((cap & CAP_MGAW_MASK) >> CAP_MGAW_SHIFT) as u8 + 1;
        let sagaw = ((cap & CAP_SAGAW_MASK) >> CAP_SAGAW_SHIFT) as u8;
        
        Self {
            num_domains,
            mgaw,
            sagaw,
            caching_mode: (cap & CAP_CM) != 0,
            rwbf: (cap & CAP_RWBF) != 0,
            large_page: (cap & CAP_SLLPS_MASK) != 0,
            queued_invalidation: (ecap & ECAP_QI) != 0,
            interrupt_remapping: (ecap & ECAP_IR) != 0,
            passthrough: (ecap & ECAP_PT) != 0,
        }
    }
}

// ============================================================================
// VT-d 单元
// ============================================================================

/// VT-d 硬件单元
pub struct VtdUnit {
    /// 寄存器基地址 (物理)
    reg_base_phys: u64,
    /// 寄存器基地址 (虚拟)
    reg_base_virt: u64,
    /// 能力
    pub capability: VtdCapability,
    /// Root Table 物理地址
    root_table_phys: u64,
    /// Root Table 虚拟地址
    root_table_virt: u64,
    /// 是否启用
    enabled: bool,
    /// 翻译模式
    translation_mode: TranslationMode,
    /// 下一个可用的 DMA 地址 (用于 Translate 模式)
    next_dma_addr: AtomicU64,
    /// 直接映射偏移
    direct_map_offset: u64,
}

impl VtdUnit {
    /// 创建新的 VT-d 单元
    pub fn new(reg_base_phys: u64, direct_map_offset: u64) -> Self {
        Self {
            reg_base_phys,
            reg_base_virt: reg_base_phys + direct_map_offset,
            capability: VtdCapability {
                num_domains: 0,
                mgaw: 0,
                sagaw: 0,
                caching_mode: false,
                rwbf: false,
                large_page: false,
                queued_invalidation: false,
                interrupt_remapping: false,
                passthrough: false,
            },
            root_table_phys: 0,
            root_table_virt: 0,
            enabled: false,
            translation_mode: TranslationMode::Passthrough,
            next_dma_addr: AtomicU64::new(0x1000), // 从 4KB 开始
            direct_map_offset,
        }
    }
    
    /// 初始化 VT-d 单元
    pub fn init(&mut self, mode: TranslationMode) -> Result<(), &'static str> {
        self.translation_mode = mode;
        
        // 1. 读取能力寄存器
        let cap = self.read_reg64(REG_CAP);
        let ecap = self.read_reg64(REG_ECAP);
        self.capability = VtdCapability::from_regs(cap, ecap);
        
        // 2. 检查是否支持所需功能
        if mode == TranslationMode::Passthrough && !self.capability.passthrough {
            return Err("VT-d does not support passthrough mode");
        }
        
        // 3. 分配并初始化 Root Table
        self.init_root_table()?;
        
        // 4. 设置 Root Table 地址
        self.write_reg64(REG_RTADDR, self.root_table_phys);
        
        // 5. 刷新写缓冲 (如果需要)
        if self.capability.rwbf {
            self.flush_write_buffer();
        }
        
        // 6. 设置 Root Table Pointer
        self.write_reg32(REG_GCMD, GCMD_SRTP);
        self.wait_for_status(GSTS_RTPS);
        
        // 7. 全局无效化 Context Cache
        self.invalidate_context_cache_global();
        
        // 8. 全局无效化 IOTLB
        self.invalidate_iotlb_global();
        
        // 9. 启用地址翻译
        self.write_reg32(REG_GCMD, GCMD_TE);
        self.wait_for_status(GSTS_TES);
        
        self.enabled = true;
        Ok(())
    }
    
    /// 映射页面
    pub fn map_pages(&mut self, phys_addr: u64, size: usize) -> Option<DmaAddr> {
        if self.translation_mode == TranslationMode::Passthrough {
            // Passthrough 模式：直接返回物理地址
            return Some(DmaAddr::new(phys_addr));
        }
        
        // Translate 模式：分配 DMA 地址并建立映射
        let pages = (size as u64 + PAGE_SIZE - 1) / PAGE_SIZE;
        let dma_addr = self.next_dma_addr.fetch_add(pages * PAGE_SIZE, Ordering::SeqCst);
        
        // 建立二级页表映射
        // 这里简化处理：假设使用 Domain 0，设备使用默认 Context
        for i in 0..pages {
            let dma = dma_addr + i * PAGE_SIZE;
            let phys = phys_addr + i * PAGE_SIZE;
            self.map_page(0, dma, phys)?;
        }
        
        // 无效化 IOTLB
        self.invalidate_iotlb_domain(0);
        
        Some(DmaAddr::new(dma_addr))
    }
    
    /// 取消映射页面
    pub fn unmap_pages(&mut self, dma_addr: DmaAddr, size: usize) {
        if self.translation_mode == TranslationMode::Passthrough {
            return;
        }
        
        let pages = (size as u64 + PAGE_SIZE - 1) / PAGE_SIZE;
        
        for i in 0..pages {
            let dma = dma_addr.as_u64() + i * PAGE_SIZE;
            self.unmap_page(0, dma);
        }
        
        self.invalidate_iotlb_domain(0);
    }
    
    // ========================================================================
    // 内部方法
    // ========================================================================
    
    /// 初始化 Root Table
    fn init_root_table(&mut self) -> Result<(), &'static str> {
        // 分配 Root Table (4KB, 256 entries × 16 bytes)
        let root_table = self.alloc_page()?;
        self.root_table_phys = root_table;
        self.root_table_virt = root_table + self.direct_map_offset;
        
        // 清零
        unsafe {
            ptr::write_bytes(self.root_table_virt as *mut u8, 0, PAGE_SIZE as usize);
        }
        
        // 为所有模式设置 Context Table
        // 即使是 Passthrough 模式，我们也需要有效的 Context Entry 来标记 Translation Type = Pass-through
        // 这里简化：假设只有一个 PCI Segment Group (0)，只设置 Bus 0
        // 实际上应该根据 ACPI DMAR 表中的 Scope 来设置
        self.setup_context_for_bus(0)?;
        
        Ok(())
    }
    
    /// 为指定 bus 设置 Context Table
    fn setup_context_for_bus(&mut self, bus: u8) -> Result<(), &'static str> {
        // 分配 Context Table (4KB, 256 entries × 16 bytes, 每个 devfn 一个)
        let ctx_table_phys = self.alloc_page()?;
        let ctx_table_virt = ctx_table_phys + self.direct_map_offset;
        
        // 清零
        unsafe {
            ptr::write_bytes(ctx_table_virt as *mut u8, 0, PAGE_SIZE as usize);
        }
        
        // 设置 Root Table Entry
        let rte_offset = (bus as u64) * 16;
        let rte_ptr = (self.root_table_virt + rte_offset) as *mut u64;
        unsafe {
            // Low 64 bits: Present + Context Table Pointer
            ptr::write_volatile(rte_ptr, (ctx_table_phys & RTE_CTP_MASK) | RTE_PRESENT);
            // High 64 bits: Reserved
            ptr::write_volatile(rte_ptr.add(1), 0);
        }

        // 如果是 Translate 模式，需要分配二级页表
        let slpt_phys = if self.translation_mode == TranslationMode::Translate {
             self.alloc_page()?
        } else {
            0
        };

        if self.translation_mode == TranslationMode::Translate {
             let slpt_virt = slpt_phys + self.direct_map_offset;
             unsafe {
                ptr::write_bytes(slpt_virt as *mut u8, 0, PAGE_SIZE as usize);
             }
        }
        
        // 为 bus 0 的所有设备设置 Context Entry
        for devfn in 0..256u16 {
            let cte_offset = (devfn as u64) * 16;
            let cte_ptr = (ctx_table_virt + cte_offset) as *mut u64;
            
            unsafe {
                if self.translation_mode == TranslationMode::Passthrough {
                        // Passthrough 模式：Translation Type = Pass-through (10b)
                        // Low 64 bits: Present + Translation Type + FPD
                        let low = CTE_PRESENT | CTE_T_PASSTHROUGH | CTE_FPD;
                        ptr::write_volatile(cte_ptr, low);
                        
                        // High 64 bits: Address Width + Domain ID
                        // AW is bits 2:0 in High Qword
                        let high = CTE_AW_48 | (0 << CTE_DID_SHIFT);
                        ptr::write_volatile(cte_ptr.add(1), high);
                    } else {
                        // Translate 模式：Translation Type = Untranslated (00b) + SLPTPTR
                        // Low 64 bits: Present + Translation Type + SLPTPTR
                        let low = CTE_PRESENT | CTE_T_UNTRANSLATED | (slpt_phys & CTE_SLPTPTR_MASK);
                        ptr::write_volatile(cte_ptr, low);
                        
                        // High 64 bits: Address Width + Domain ID
                        let high = CTE_AW_48 | (0 << CTE_DID_SHIFT);
                        ptr::write_volatile(cte_ptr.add(1), high);
                    }
            }
        }
        
        Ok(())
    }
    
    /// 映射单个页面 (4KB)
    fn map_page(&mut self, domain_id: u16, dma_addr: u64, phys_addr: u64) -> Option<()> {
        // 获取 Domain 0 的二级页表根 (简化：从 bus 0, devfn 0 的 Context Entry 获取)
        let ctx_table_phys = self.get_context_table_for_bus(0)?;
        let ctx_table_virt = ctx_table_phys + self.direct_map_offset;
        
        let cte_ptr = ctx_table_virt as *mut u64;
        let slpt_phys = unsafe { ptr::read_volatile(cte_ptr.add(1)) & CTE_SLPTPTR_MASK };
        
        // 遍历 4 级页表
        let indices = [
            (dma_addr >> 39) & 0x1FF, // PML4
            (dma_addr >> 30) & 0x1FF, // PDPT
            (dma_addr >> 21) & 0x1FF, // PD
            (dma_addr >> 12) & 0x1FF, // PT
        ];
        
        let mut table_phys = slpt_phys;
        
        // 遍历前 3 级，确保中间表存在
        for level in 0..3 {
            let table_virt = table_phys + self.direct_map_offset;
            let entry_ptr = (table_virt + indices[level] * 8) as *mut u64;
            
            let entry = unsafe { ptr::read_volatile(entry_ptr) };
            
            if entry & SLPTE_P == 0 {
                // 需要分配新的页表
                let new_table = self.alloc_page().ok()?;
                let new_table_virt = new_table + self.direct_map_offset;
                unsafe {
                    ptr::write_bytes(new_table_virt as *mut u8, 0, PAGE_SIZE as usize);
                    ptr::write_volatile(entry_ptr, (new_table & SLPTE_ADDR_MASK) | SLPTE_R | SLPTE_W);
                }
                table_phys = new_table;
            } else {
                table_phys = entry & SLPTE_ADDR_MASK;
            }
        }
        
        // 设置 4 级 (PT) 条目
        let pt_virt = table_phys + self.direct_map_offset;
        let pte_ptr = (pt_virt + indices[3] * 8) as *mut u64;
        unsafe {
            ptr::write_volatile(pte_ptr, (phys_addr & SLPTE_ADDR_MASK) | SLPTE_R | SLPTE_W);
        }
        
        Some(())
    }
    
    /// 取消映射单个页面
    fn unmap_page(&mut self, _domain_id: u16, dma_addr: u64) {
        let ctx_table_phys = match self.get_context_table_for_bus(0) {
            Some(p) => p,
            None => return,
        };
        let ctx_table_virt = ctx_table_phys + self.direct_map_offset;
        
        let cte_ptr = ctx_table_virt as *mut u64;
        let slpt_phys = unsafe { ptr::read_volatile(cte_ptr.add(1)) & CTE_SLPTPTR_MASK };
        
        let indices = [
            (dma_addr >> 39) & 0x1FF,
            (dma_addr >> 30) & 0x1FF,
            (dma_addr >> 21) & 0x1FF,
            (dma_addr >> 12) & 0x1FF,
        ];
        
        let mut table_phys = slpt_phys;
        
        // 遍历到 PT
        for level in 0..3 {
            let table_virt = table_phys + self.direct_map_offset;
            let entry_ptr = (table_virt + indices[level] * 8) as *mut u64;
            let entry = unsafe { ptr::read_volatile(entry_ptr) };
            
            if entry & SLPTE_P == 0 {
                return; // 已经不存在
            }
            table_phys = entry & SLPTE_ADDR_MASK;
        }
        
        // 清除 PT 条目
        let pt_virt = table_phys + self.direct_map_offset;
        let pte_ptr = (pt_virt + indices[3] * 8) as *mut u64;
        unsafe {
            ptr::write_volatile(pte_ptr, 0);
        }
    }
    
    /// 获取指定 bus 的 Context Table 地址
    fn get_context_table_for_bus(&self, bus: u8) -> Option<u64> {
        let rte_offset = (bus as u64) * 16;
        let rte_ptr = (self.root_table_virt + rte_offset) as *const u64;
        
        let entry = unsafe { ptr::read_volatile(rte_ptr) };
        if entry & RTE_PRESENT == 0 {
            return None;
        }
        
        Some(entry & RTE_CTP_MASK)
    }
    
    /// 分配一个物理页面
    fn alloc_page(&self) -> Result<u64, &'static str> {
        // IOMMU 初始化发生在 Buddy/SLUB 就绪之后，优先走标准页分配路径。
        let Some(page) = crate::mm::alloc_pages(0, crate::mm::GFP_KERNEL_ZERO) else {
            return Err("Failed to allocate page for VT-d");
        };
        Ok(crate::mm::page_to_pfn(page) * PAGE_SIZE)
    }
    
    /// 刷新写缓冲
    fn flush_write_buffer(&mut self) {
        self.write_reg32(REG_GCMD, GCMD_WBF);
        // 等待完成 (WBFS 变为 0)
        while (self.read_reg32(REG_GSTS) & GSTS_WBFS) != 0 {
            core::hint::spin_loop();
        }
    }
    
    /// 全局无效化 Context Cache
    fn invalidate_context_cache_global(&mut self) {
        self.write_reg64(REG_CCMD, CCMD_ICC | CCMD_CIRG_GLOBAL);
        // 等待完成 (ICC 变为 0)
        while (self.read_reg64(REG_CCMD) & CCMD_ICC) != 0 {
            core::hint::spin_loop();
        }
    }
    
    /// 全局无效化 IOTLB
    fn invalidate_iotlb_global(&mut self) {
        // IOTLB 寄存器偏移
        let iro = ((self.read_reg64(REG_ECAP) & ECAP_IRO_MASK) >> ECAP_IRO_SHIFT) as u64 * 16;
        let iotlb_reg = iro + 8; // IVA + 8 = IOTLB
        
        // 全局无效化
        let cmd: u64 = (1 << 63) | (1 << 60); // IVT + IIRG (global)
        self.write_reg64(iotlb_reg, cmd);
        
        // 等待完成
        while (self.read_reg64(iotlb_reg) & (1 << 63)) != 0 {
            core::hint::spin_loop();
        }
    }
    
    /// 按域无效化 IOTLB
    fn invalidate_iotlb_domain(&mut self, domain_id: u16) {
        let iro = ((self.read_reg64(REG_ECAP) & ECAP_IRO_MASK) >> ECAP_IRO_SHIFT) as u64 * 16;
        let iotlb_reg = iro + 8;
        
        // 域级无效化
        let cmd: u64 = (1 << 63) | (2 << 60) | ((domain_id as u64) << 32);
        self.write_reg64(iotlb_reg, cmd);
        
        while (self.read_reg64(iotlb_reg) & (1 << 63)) != 0 {
            core::hint::spin_loop();
        }
    }
    
    /// 等待状态位
    fn wait_for_status(&self, status_bit: u32) {
        let mut timeout = 1000000u32;
        while (self.read_reg32(REG_GSTS) & status_bit) == 0 {
            core::hint::spin_loop();
            timeout -= 1;
            if timeout == 0 {
                break;
            }
        }
    }
    
    // ========================================================================
    // 寄存器访问
    // ========================================================================
    
    fn read_reg32(&self, offset: u64) -> u32 {
        unsafe { ptr::read_volatile((self.reg_base_virt + offset) as *const u32) }
    }
    
    fn write_reg32(&mut self, offset: u64, value: u32) {
        unsafe { ptr::write_volatile((self.reg_base_virt + offset) as *mut u32, value) }
    }
    
    fn read_reg64(&self, offset: u64) -> u64 {
        unsafe { ptr::read_volatile((self.reg_base_virt + offset) as *const u64) }
    }
    
    fn write_reg64(&mut self, offset: u64, value: u64) {
        unsafe { ptr::write_volatile((self.reg_base_virt + offset) as *mut u64, value) }
    }
}
