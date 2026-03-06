use std::env;
use std::fs;
use std::path::PathBuf;

const DEFAULT_DIRECT_MAP_OFFSET: &str = "0xFFFF880000000000";
const DEFAULT_VMALLOC_START: &str = "0xFFFFC90000000000";
const DEFAULT_VMALLOC_END: &str = "0xFFFFE8FFFFFFFFFF";
const DEFAULT_INITRD_COMMAND: &str = "/bin/sh";
const DEFAULT_VMEMMAP_START: &str = "0xFFFFEA0000000000";
const DEFAULT_VMEMMAP_END: &str = "0xFFFFEFFFFFFFFFFF";
const DEFAULT_MODULES_START: &str = "0xFFFFFFFFA0000000";
const DEFAULT_MODULES_END: &str = "0xFFFFFFFFFEFFFFFF";
const DEFAULT_FIXMAP_START: &str = "0xFFFFFFFFFF000000";
const DEFAULT_FIXMAP_END: &str = "0xFFFFFFFFFFFFF000";
const DEFAULT_VA_MODE: &str = "la57_prefer";
const DEFAULT_LA57_FALLBACK: &str = "4level";
const DEFAULT_MANAGE_FULL_PHYS: bool = true;

struct KernelLayoutCfg {
    direct_map_offset: u64,
    vmalloc_start: u64,
    vmalloc_end: u64,
    initrd_command: String,
    vmemmap_start: u64,
    vmemmap_end: u64,
    modules_start: u64,
    modules_end: u64,
    fixmap_start: u64,
    fixmap_end: u64,
    va_mode: String,
    la57_fallback: String,
    manage_full_phys: bool,
}

fn parse_u64_literal(raw: &str, field: &str) -> Result<u64, String> {
    let s = raw.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).map_err(|e| format!("invalid {} '{}': {}", field, raw, e))
    } else {
        s.parse::<u64>()
            .map_err(|e| format!("invalid {} '{}': {}", field, raw, e))
    }
}

fn parse_toml_string_value(raw: &str) -> Option<&str> {
    let mut s = raw.trim();
    if let Some((head, _)) = s.split_once('#') {
        s = head.trim();
    }
    if s.is_empty() {
        return None;
    }
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        return Some(&s[1..s.len() - 1]);
    }
    Some(s)
}

fn parse_toml_bool_value(raw: &str) -> Option<bool> {
    let mut s = raw.trim();
    if let Some((head, _)) = s.split_once('#') {
        s = head.trim();
    }
    if s.eq_ignore_ascii_case("true") {
        return Some(true);
    }
    if s.eq_ignore_ascii_case("false") {
        return Some(false);
    }
    None
}

fn escape_rust_string(raw: &str) -> String {
    raw.replace('\\', "\\\\").replace('"', "\\\"")
}

fn kernel_layout_from_cfg(path: &PathBuf) -> Result<KernelLayoutCfg, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
    let mut in_kernel = false;
    let mut in_kernel_layout = false;
    let mut direct_map_offset = DEFAULT_DIRECT_MAP_OFFSET.to_string();
    let mut vmalloc_start = DEFAULT_VMALLOC_START.to_string();
    let mut vmalloc_end = DEFAULT_VMALLOC_END.to_string();
    let mut initrd_command = DEFAULT_INITRD_COMMAND.to_string();
    let mut vmemmap_start = DEFAULT_VMEMMAP_START.to_string();
    let mut vmemmap_end = DEFAULT_VMEMMAP_END.to_string();
    let mut modules_start = DEFAULT_MODULES_START.to_string();
    let mut modules_end = DEFAULT_MODULES_END.to_string();
    let mut fixmap_start = DEFAULT_FIXMAP_START.to_string();
    let mut fixmap_end = DEFAULT_FIXMAP_END.to_string();
    let mut va_mode = DEFAULT_VA_MODE.to_string();
    let mut la57_fallback = DEFAULT_LA57_FALLBACK.to_string();
    let mut manage_full_phys = DEFAULT_MANAGE_FULL_PHYS;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_kernel = trimmed == "[kernel]";
            in_kernel_layout = trimmed == "[kernel.layout]";
            continue;
        }
        if !in_kernel && !in_kernel_layout {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if in_kernel {
            let Some(value) = parse_toml_string_value(value) else {
                continue;
            };
            match key {
                "direct_map_offset" => direct_map_offset = value.to_string(),
                "vmalloc_start" => vmalloc_start = value.to_string(),
                "vmalloc_end" => vmalloc_end = value.to_string(),
                "initrd_command" => initrd_command = value.to_string(),
                _ => {}
            }
        } else if in_kernel_layout {
            match key {
                "manage_full_phys" => {
                    if let Some(v) = parse_toml_bool_value(value) {
                        manage_full_phys = v;
                    }
                }
                _ => {
                    let Some(value) = parse_toml_string_value(value) else {
                        continue;
                    };
                    match key {
                        "va_mode" => va_mode = value.to_string(),
                        "la57_fallback" => la57_fallback = value.to_string(),
                        "vmemmap_start" => vmemmap_start = value.to_string(),
                        "vmemmap_end" => vmemmap_end = value.to_string(),
                        "modules_start" => modules_start = value.to_string(),
                        "modules_end" => modules_end = value.to_string(),
                        "fixmap_start" => fixmap_start = value.to_string(),
                        "fixmap_end" => fixmap_end = value.to_string(),
                        _ => {}
                    }
                }
            }
        }
    }

    let direct_map_offset = parse_u64_literal(&direct_map_offset, "kernel.direct_map_offset")?;
    let vmalloc_start = parse_u64_literal(&vmalloc_start, "kernel.vmalloc_start")?;
    let vmalloc_end = parse_u64_literal(&vmalloc_end, "kernel.vmalloc_end")?;
    let vmemmap_start = parse_u64_literal(&vmemmap_start, "kernel.layout.vmemmap_start")?;
    let vmemmap_end = parse_u64_literal(&vmemmap_end, "kernel.layout.vmemmap_end")?;
    let modules_start = parse_u64_literal(&modules_start, "kernel.layout.modules_start")?;
    let modules_end = parse_u64_literal(&modules_end, "kernel.layout.modules_end")?;
    let fixmap_start = parse_u64_literal(&fixmap_start, "kernel.layout.fixmap_start")?;
    let fixmap_end = parse_u64_literal(&fixmap_end, "kernel.layout.fixmap_end")?;

    if direct_map_offset >= vmalloc_start {
        return Err(format!(
            "invalid layout: direct_map_offset ({:#x}) must be below vmalloc_start ({:#x})",
            direct_map_offset, vmalloc_start
        ));
    }
    if vmalloc_start >= vmalloc_end {
        return Err(format!(
            "invalid layout: vmalloc_start ({:#x}) must be below vmalloc_end ({:#x})",
            vmalloc_start, vmalloc_end
        ));
    }
    if !(vmemmap_start < vmemmap_end && modules_start < modules_end && fixmap_start < fixmap_end) {
        return Err(
            "invalid layout: vmemmap/modules/fixmap window start must be below end".to_string(),
        );
    }
    if initrd_command.is_empty() {
        return Err("invalid kernel.initrd_command: must not be empty".to_string());
    }
    if initrd_command
        .as_bytes()
        .iter()
        .any(u8::is_ascii_whitespace)
    {
        return Err(
            "invalid kernel.initrd_command: must not contain whitespace (cmdline token)"
                .to_string(),
        );
    }

    Ok(KernelLayoutCfg {
        direct_map_offset,
        vmalloc_start,
        vmalloc_end,
        initrd_command,
        vmemmap_start,
        vmemmap_end,
        modules_start,
        modules_end,
        fixmap_start,
        fixmap_end,
        va_mode,
        la57_fallback,
        manage_full_phys,
    })
}

fn main() {
    println!("cargo:rerun-if-env-changed=OS_CFG_PATH");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let default_cfg = manifest_dir.join("../../os_cfg.toml");
    let cfg_path = env::var("OS_CFG_PATH")
        .map(PathBuf::from)
        .unwrap_or(default_cfg);
    println!("cargo:rerun-if-changed={}", cfg_path.display());

    let cfg = kernel_layout_from_cfg(&cfg_path)
        .unwrap_or_else(|e| panic!("boot layout config error: {}", e));

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let out = out_dir.join("os_cfg.rs");
    let generated = format!(
        "// Auto-generated by boot/x86_64/build.rs - DO NOT EDIT\n\
         pub const DIRECT_MAP_OFFSET: u64 = {:#x};\n\
         pub const VMALLOC_START: u64 = {:#x};\n\
         pub const VMALLOC_END: u64 = {:#x};\n\
         pub const INITRD_COMMAND: &str = \"{}\";\n\
         pub const VMEMMAP_START: u64 = {:#x};\n\
         pub const VMEMMAP_END: u64 = {:#x};\n\
         pub const MODULES_START: u64 = {:#x};\n\
         pub const MODULES_END: u64 = {:#x};\n\
         pub const FIXMAP_START: u64 = {:#x};\n\
         pub const FIXMAP_END: u64 = {:#x};\n\
         pub const KERNEL_MANAGE_FULL_PHYS: bool = {};\n\
         pub const KERNEL_VA_MODE: &str = \"{}\";\n\
         pub const KERNEL_LA57_FALLBACK: &str = \"{}\";\n",
        cfg.direct_map_offset,
        cfg.vmalloc_start,
        cfg.vmalloc_end,
        escape_rust_string(&cfg.initrd_command),
        cfg.vmemmap_start,
        cfg.vmemmap_end,
        cfg.modules_start,
        cfg.modules_end,
        cfg.fixmap_start,
        cfg.fixmap_end,
        cfg.manage_full_phys,
        cfg.va_mode,
        cfg.la57_fallback
    );
    fs::write(&out, generated).unwrap_or_else(|e| {
        panic!("failed to write {}: {}", out.display(), e);
    });
}
