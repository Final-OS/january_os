//! Build-time generated layout config from `os_cfg.toml`.
#![allow(dead_code)]

include!(concat!(env!("OUT_DIR"), "/os_cfg.rs"));
