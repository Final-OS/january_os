pub mod context;
pub mod switch;
pub mod user;

/// ELF e_machine for x86_64 (EM_X86_64).
const ELF_MACHINE: u16 = 62;

#[inline]
pub const fn supported_elf_machine() -> u16 {
    ELF_MACHINE
}

pub use context::TaskContext;
pub use switch::__switch;

pub use user::{
    build_user_enter_frame, enter_user_fork_return, enter_user_mode_iret, ForkReturnFrame,
    UserEnterFrame,
};
