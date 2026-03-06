use alloc::string::String;

pub fn dump_state() -> String {
    crate::fs::runtime::manager::dump_state()
}
