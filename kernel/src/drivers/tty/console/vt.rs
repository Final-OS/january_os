//! VT100/ANSI 转义序列解析器
//!
//! 支持常用的 ANSI 转义序列：
//! - 光标移动 (CUU, CUD, CUF, CUB, CUP)
//! - 擦除 (ED, EL)
//! - 颜色/属性 (SGR)
//! - 滚动 (SU, SD)
//! - 光标显示/隐藏

/// VT 解析器状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VtState {
    /// 普通文本
    Ground,
    /// 收到 ESC
    Escape,
    /// CSI 序列 (ESC [)
    Csi,
    /// CSI 参数
    CsiParam,
    /// OSC 序列 (ESC ])
    Osc,
    /// DCS 序列 (ESC P)
    Dcs,
}

/// VT 动作
#[derive(Debug, Clone, Copy)]
pub enum VtAction {
    /// 打印字符
    Print(char),
    /// 光标上移
    CursorUp(usize),
    /// 光标下移
    CursorDown(usize),
    /// 光标右移
    CursorForward(usize),
    /// 光标左移
    CursorBack(usize),
    /// 光标定位 (行, 列) - 1-based
    CursorPosition(usize, usize),
    /// 擦除显示 (0=光标到末尾, 1=开头到光标, 2=全屏, 3=全屏+滚动缓冲)
    EraseDisplay(u8),
    /// 擦除行 (0=光标到行尾, 1=行首到光标, 2=整行)
    EraseLine(u8),
    /// 设置属性 (SGR)
    SetAttr(u8),
    /// 向上滚动
    ScrollUp(usize),
    /// 向下滚动
    ScrollDown(usize),
    /// 保存光标
    SaveCursor,
    /// 恢复光标
    RestoreCursor,
    /// 显示光标
    ShowCursor,
    /// 隐藏光标
    HideCursor,
    /// 响铃
    Bell,
    /// 重置
    Reset,
}

/// VT 解析器
pub struct VtParser {
    /// 当前状态
    state: VtState,
    /// 参数缓冲区
    params: [u16; 16],
    /// 参数数量
    param_count: usize,
    /// 当前参数值
    current_param: u16,
    /// 私有模式标志
    private_mode: bool,
    /// 中间字符
    intermediate: u8,
}

impl VtParser {
    /// 创建新的解析器
    pub const fn new() -> Self {
        Self {
            state: VtState::Ground,
            params: [0; 16],
            param_count: 0,
            current_param: 0,
            private_mode: false,
            intermediate: 0,
        }
    }
    
    /// 重置解析器
    pub fn reset(&mut self) {
        self.state = VtState::Ground;
        self.param_count = 0;
        self.current_param = 0;
        self.private_mode = false;
        self.intermediate = 0;
    }
    
    /// 输入字符，返回动作迭代器
    pub fn feed(&mut self, ch: char) -> VtActionIter {
        let mut actions = VtActionIter::new();
        
        match self.state {
            VtState::Ground => {
                match ch {
                    '\x1B' => {
                        // ESC
                        self.state = VtState::Escape;
                    }
                    '\x07' => {
                        // BEL
                        actions.push(VtAction::Bell);
                    }
                    '\x08' => {
                        // BS
                        actions.push(VtAction::CursorBack(1));
                    }
                    '\x09' => {
                        // TAB
                        actions.push(VtAction::Print('\t'));
                    }
                    '\x0A' | '\x0B' | '\x0C' => {
                        // LF, VT, FF
                        actions.push(VtAction::Print('\n'));
                    }
                    '\x0D' => {
                        // CR
                        actions.push(VtAction::Print('\r'));
                    }
                    _ => {
                        // 可打印字符
                        if ch >= ' ' {
                            actions.push(VtAction::Print(ch));
                        }
                    }
                }
            }
            VtState::Escape => {
                match ch {
                    '[' => {
                        // CSI
                        self.state = VtState::Csi;
                        self.param_count = 0;
                        self.current_param = 0;
                        self.private_mode = false;
                        self.intermediate = 0;
                    }
                    ']' => {
                        // OSC
                        self.state = VtState::Osc;
                    }
                    'P' => {
                        // DCS
                        self.state = VtState::Dcs;
                    }
                    'c' => {
                        // RIS - 重置
                        actions.push(VtAction::Reset);
                        self.reset();
                    }
                    '7' => {
                        // DECSC - 保存光标
                        actions.push(VtAction::SaveCursor);
                        self.state = VtState::Ground;
                    }
                    '8' => {
                        // DECRC - 恢复光标
                        actions.push(VtAction::RestoreCursor);
                        self.state = VtState::Ground;
                    }
                    'D' => {
                        // IND - 下移
                        actions.push(VtAction::CursorDown(1));
                        self.state = VtState::Ground;
                    }
                    'E' => {
                        // NEL - 新行
                        actions.push(VtAction::Print('\r'));
                        actions.push(VtAction::Print('\n'));
                        self.state = VtState::Ground;
                    }
                    'M' => {
                        // RI - 上移
                        actions.push(VtAction::CursorUp(1));
                        self.state = VtState::Ground;
                    }
                    _ => {
                        // 未知序列，忽略
                        self.state = VtState::Ground;
                    }
                }
            }
            VtState::Csi => {
                match ch {
                    '?' => {
                        // 私有模式
                        self.private_mode = true;
                        self.state = VtState::CsiParam;
                    }
                    '0'..='9' => {
                        self.current_param = (ch as u16) - ('0' as u16);
                        self.state = VtState::CsiParam;
                    }
                    ';' => {
                        self.push_param();
                        self.state = VtState::CsiParam;
                    }
                    _ => {
                        // 直接是终结符
                        self.push_param();
                        self.execute_csi(ch, &mut actions);
                        self.state = VtState::Ground;
                    }
                }
            }
            VtState::CsiParam => {
                match ch {
                    '0'..='9' => {
                        self.current_param = self.current_param
                            .saturating_mul(10)
                            .saturating_add((ch as u16) - ('0' as u16));
                    }
                    ';' => {
                        self.push_param();
                    }
                    ' ' | '!' | '"' | '#' | '$' | '%' | '&' | '\'' => {
                        // 中间字符
                        self.intermediate = ch as u8;
                    }
                    _ => {
                        // 终结符
                        self.push_param();
                        self.execute_csi(ch, &mut actions);
                        self.state = VtState::Ground;
                    }
                }
            }
            VtState::Osc => {
                // OSC 序列以 BEL 或 ST (ESC \) 结束
                match ch {
                    '\x07' => {
                        // BEL 终止
                        self.state = VtState::Ground;
                    }
                    '\x1B' => {
                        // 可能是 ST
                        // 简化处理：忽略 OSC 内容
                    }
                    '\\' => {
                        // ST 终止
                        self.state = VtState::Ground;
                    }
                    _ => {
                        // 忽略 OSC 内容
                    }
                }
            }
            VtState::Dcs => {
                // DCS 序列以 ST 结束
                match ch {
                    '\x1B' => {
                        // 可能是 ST
                    }
                    '\\' => {
                        self.state = VtState::Ground;
                    }
                    _ => {
                        // 忽略 DCS 内容
                    }
                }
            }
        }
        
        actions
    }
    
    /// 推送参数
    fn push_param(&mut self) {
        if self.param_count < self.params.len() {
            self.params[self.param_count] = self.current_param;
            self.param_count += 1;
        }
        self.current_param = 0;
    }
    
    /// 获取参数 (带默认值)
    fn get_param(&self, index: usize, default: u16) -> u16 {
        if index < self.param_count && self.params[index] > 0 {
            self.params[index]
        } else {
            default
        }
    }
    
    /// 执行 CSI 序列
    fn execute_csi(&self, ch: char, actions: &mut VtActionIter) {
        match ch {
            'A' => {
                // CUU - 光标上移
                let n = self.get_param(0, 1) as usize;
                actions.push(VtAction::CursorUp(n));
            }
            'B' => {
                // CUD - 光标下移
                let n = self.get_param(0, 1) as usize;
                actions.push(VtAction::CursorDown(n));
            }
            'C' => {
                // CUF - 光标右移
                let n = self.get_param(0, 1) as usize;
                actions.push(VtAction::CursorForward(n));
            }
            'D' => {
                // CUB - 光标左移
                let n = self.get_param(0, 1) as usize;
                actions.push(VtAction::CursorBack(n));
            }
            'E' => {
                // CNL - 光标下移到行首
                let n = self.get_param(0, 1) as usize;
                actions.push(VtAction::CursorDown(n));
                actions.push(VtAction::Print('\r'));
            }
            'F' => {
                // CPL - 光标上移到行首
                let n = self.get_param(0, 1) as usize;
                actions.push(VtAction::CursorUp(n));
                actions.push(VtAction::Print('\r'));
            }
            'G' => {
                // CHA - 光标到列
                let col = self.get_param(0, 1) as usize;
                actions.push(VtAction::CursorPosition(0, col));
            }
            'H' | 'f' => {
                // CUP/HVP - 光标定位
                let row = self.get_param(0, 1) as usize;
                let col = self.get_param(1, 1) as usize;
                actions.push(VtAction::CursorPosition(row, col));
            }
            'J' => {
                // ED - 擦除显示
                let mode = self.get_param(0, 0) as u8;
                actions.push(VtAction::EraseDisplay(mode));
            }
            'K' => {
                // EL - 擦除行
                let mode = self.get_param(0, 0) as u8;
                actions.push(VtAction::EraseLine(mode));
            }
            'S' => {
                // SU - 向上滚动
                let n = self.get_param(0, 1) as usize;
                actions.push(VtAction::ScrollUp(n));
            }
            'T' => {
                // SD - 向下滚动
                let n = self.get_param(0, 1) as usize;
                actions.push(VtAction::ScrollDown(n));
            }
            'm' => {
                // SGR - 设置属性
                if self.param_count == 0 {
                    actions.push(VtAction::SetAttr(0));
                } else {
                    for i in 0..self.param_count {
                        actions.push(VtAction::SetAttr(self.params[i] as u8));
                    }
                }
            }
            's' => {
                // SCP - 保存光标位置
                actions.push(VtAction::SaveCursor);
            }
            'u' => {
                // RCP - 恢复光标位置
                actions.push(VtAction::RestoreCursor);
            }
            'h' => {
                // SM - 设置模式
                if self.private_mode {
                    let mode = self.get_param(0, 0);
                    match mode {
                        25 => actions.push(VtAction::ShowCursor),
                        _ => {}
                    }
                }
            }
            'l' => {
                // RM - 重置模式
                if self.private_mode {
                    let mode = self.get_param(0, 0);
                    match mode {
                        25 => actions.push(VtAction::HideCursor),
                        _ => {}
                    }
                }
            }
            'n' => {
                // DSR - 设备状态报告
                // TODO: 需要响应
            }
            'c' => {
                // DA - 设备属性
                // TODO: 需要响应
            }
            _ => {
                // 未知序列，忽略
            }
        }
    }
}

/// VT 动作迭代器
pub struct VtActionIter {
    actions: [Option<VtAction>; 8],
    count: usize,
    index: usize,
}

impl VtActionIter {
    fn new() -> Self {
        Self {
            actions: [None; 8],
            count: 0,
            index: 0,
        }
    }
    
    fn push(&mut self, action: VtAction) {
        if self.count < self.actions.len() {
            self.actions[self.count] = Some(action);
            self.count += 1;
        }
    }
}

impl Iterator for VtActionIter {
    type Item = VtAction;
    
    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.count {
            let action = self.actions[self.index].take();
            self.index += 1;
            action
        } else {
            None
        }
    }
}
