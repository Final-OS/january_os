// ============================================================================
// january_os - PS/2 键盘驱动
// ============================================================================

use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

// ============================================================================
// 键盘扫描码表 (Set 1)
// ============================================================================

/// 扫描码到 ASCII 映射 (无 Shift) - 128 bytes
#[rustfmt::skip]
const SCANCODE_TO_ASCII: [u8; 128] = [
//  0     1     2     3     4     5     6     7     8     9     A     B     C     D     E     F
    0,   27, b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'0', b'-', b'=',   8,b'\t', // 0x0_
  b'q', b'w', b'e', b'r', b't', b'y', b'u', b'i', b'o', b'p', b'[', b']',b'\n',   0, b'a', b's', // 0x1_
  b'd', b'f', b'g', b'h', b'j', b'k', b'l', b';',b'\'', b'`',   0,b'\\', b'z', b'x', b'c', b'v', // 0x2_
  b'b', b'n', b'm', b',', b'.', b'/',   0, b'*',   0, b' ',   0,   0,   0,   0,   0,   0, // 0x3_
    0,    0,   0,   0,   0,   0,   0, b'7', b'8', b'9', b'-', b'4', b'5', b'6', b'+', b'1', // 0x4_
  b'2', b'3', b'0', b'.',   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0, // 0x5_
    0,    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0, // 0x6_
    0,    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0, // 0x7_
];

/// 扫描码到 ASCII 映射 (有 Shift) - 128 bytes
#[rustfmt::skip]
const SCANCODE_TO_ASCII_SHIFT: [u8; 128] = [
//  0     1     2     3     4     5     6     7     8     9     A     B     C     D     E     F
    0,   27, b'!', b'@', b'#', b'$', b'%', b'^', b'&', b'*', b'(', b')', b'_', b'+',   8,b'\t', // 0x0_
  b'Q', b'W', b'E', b'R', b'T', b'Y', b'U', b'I', b'O', b'P', b'{', b'}',b'\n',   0, b'A', b'S', // 0x1_
  b'D', b'F', b'G', b'H', b'J', b'K', b'L', b':', b'"', b'~',   0, b'|', b'Z', b'X', b'C', b'V', // 0x2_
  b'B', b'N', b'M', b'<', b'>', b'?',   0, b'*',   0, b' ',   0,   0,   0,   0,   0,   0, // 0x3_
    0,    0,   0,   0,   0,   0,   0, b'7', b'8', b'9', b'-', b'4', b'5', b'6', b'+', b'1', // 0x4_
  b'2', b'3', b'0', b'.',   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0, // 0x5_
    0,    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0, // 0x6_
    0,    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0, // 0x7_
];

// ============================================================================
// 特殊键扫描码
// ============================================================================

const KEY_LSHIFT: u8 = 0x2A;
const KEY_RSHIFT: u8 = 0x36;
const KEY_LCTRL: u8 = 0x1D;
const KEY_LALT: u8 = 0x38;
const KEY_CAPS_LOCK: u8 = 0x3A;
const KEY_ESC: u8 = 0x01;
const KEY_BACKSPACE: u8 = 0x0E;
const KEY_ENTER: u8 = 0x1C;
const KEY_TAB: u8 = 0x0F;

// F1-F12 键
const KEY_F1: u8 = 0x3B;
const KEY_F12: u8 = 0x58;

// ============================================================================
// 键盘状态
// ============================================================================

/// 修饰键状态
static SHIFT_PRESSED: AtomicU8 = AtomicU8::new(0);
static CTRL_PRESSED: AtomicU8 = AtomicU8::new(0);
static ALT_PRESSED: AtomicU8 = AtomicU8::new(0);
static CAPS_LOCK: AtomicU8 = AtomicU8::new(0);

/// 输入缓冲区
const BUFFER_SIZE: usize = 64;
static BUFFER: [AtomicU8; BUFFER_SIZE] = {
    const INIT: AtomicU8 = AtomicU8::new(0);
    [INIT; BUFFER_SIZE]
};
static BUFFER_HEAD: AtomicUsize = AtomicUsize::new(0);
static BUFFER_TAIL: AtomicUsize = AtomicUsize::new(0);

/// 最后按下的键 (用于显示)
static LAST_SCANCODE: AtomicU8 = AtomicU8::new(0);
static LAST_CHAR: AtomicU8 = AtomicU8::new(0);

// ============================================================================
// 键盘操作
// ============================================================================

/// 处理扫描码
/// 
/// 由键盘中断处理程序调用
pub fn handle_scancode(scancode: u8) {
    LAST_SCANCODE.store(scancode, Ordering::Relaxed);
    
    // 检查是否是释放键 (bit 7 set)
    let released = scancode & 0x80 != 0;
    let key = scancode & 0x7F;
    
    // 处理修饰键
    match key {
        KEY_LSHIFT | KEY_RSHIFT => {
            if released {
                SHIFT_PRESSED.store(0, Ordering::Relaxed);
            } else {
                SHIFT_PRESSED.store(1, Ordering::Relaxed);
            }
            return;
        }
        KEY_LCTRL => {
            if released {
                CTRL_PRESSED.store(0, Ordering::Relaxed);
            } else {
                CTRL_PRESSED.store(1, Ordering::Relaxed);
            }
            return;
        }
        KEY_LALT => {
            if released {
                ALT_PRESSED.store(0, Ordering::Relaxed);
            } else {
                ALT_PRESSED.store(1, Ordering::Relaxed);
            }
            return;
        }
        KEY_CAPS_LOCK => {
            if !released {
                // 切换 Caps Lock
                let current = CAPS_LOCK.load(Ordering::Relaxed);
                CAPS_LOCK.store(1 - current, Ordering::Relaxed);
            }
            return;
        }
        _ => {}
    }
    
    // 只处理按下事件
    if released {
        return;
    }
    
    // 转换为 ASCII
    let shift = SHIFT_PRESSED.load(Ordering::Relaxed) != 0;
    let caps = CAPS_LOCK.load(Ordering::Relaxed) != 0;
    
    let ascii = if key < 128 {
        let base_char = if shift {
            SCANCODE_TO_ASCII_SHIFT[key as usize]
        } else {
            SCANCODE_TO_ASCII[key as usize]
        };
        
        // Caps Lock 只影响字母
        if caps && base_char >= b'a' && base_char <= b'z' {
            base_char - 32 // 转大写
        } else if caps && base_char >= b'A' && base_char <= b'Z' {
            base_char + 32 // 转小写
        } else {
            base_char
        }
    } else {
        0
    };
    
    LAST_CHAR.store(ascii, Ordering::Relaxed);
    
    // 将字符放入缓冲区
    if ascii != 0 {
        push_char(ascii);
    }
}

/// 将字符放入缓冲区
fn push_char(c: u8) {
    let head = BUFFER_HEAD.load(Ordering::Relaxed);
    let next_head = (head + 1) % BUFFER_SIZE;
    
    // 检查缓冲区是否已满
    if next_head == BUFFER_TAIL.load(Ordering::Relaxed) {
        return; // 缓冲区满，丢弃字符
    }
    
    BUFFER[head].store(c, Ordering::Relaxed);
    BUFFER_HEAD.store(next_head, Ordering::Relaxed);
}

/// 从缓冲区读取字符
pub fn read_char() -> Option<u8> {
    let tail = BUFFER_TAIL.load(Ordering::Relaxed);
    let head = BUFFER_HEAD.load(Ordering::Relaxed);
    
    if tail == head {
        return None; // 缓冲区空
    }
    
    let c = BUFFER[tail].load(Ordering::Relaxed);
    BUFFER_TAIL.store((tail + 1) % BUFFER_SIZE, Ordering::Relaxed);
    Some(c)
}

/// 检查是否有字符可读
pub fn has_char() -> bool {
    BUFFER_TAIL.load(Ordering::Relaxed) != BUFFER_HEAD.load(Ordering::Relaxed)
}

/// 获取缓冲区中的字符数量
pub fn buffer_len() -> usize {
    let head = BUFFER_HEAD.load(Ordering::Relaxed);
    let tail = BUFFER_TAIL.load(Ordering::Relaxed);
    if head >= tail {
        head - tail
    } else {
        BUFFER_SIZE - tail + head
    }
}

/// 获取最后按下的扫描码
pub fn last_scancode() -> u8 {
    LAST_SCANCODE.load(Ordering::Relaxed)
}

/// 获取最后输入的字符
pub fn last_char() -> u8 {
    LAST_CHAR.load(Ordering::Relaxed)
}

/// 检查 Shift 是否按下
pub fn is_shift_pressed() -> bool {
    SHIFT_PRESSED.load(Ordering::Relaxed) != 0
}

/// 检查 Ctrl 是否按下
pub fn is_ctrl_pressed() -> bool {
    CTRL_PRESSED.load(Ordering::Relaxed) != 0
}

/// 检查 Alt 是否按下
pub fn is_alt_pressed() -> bool {
    ALT_PRESSED.load(Ordering::Relaxed) != 0
}
