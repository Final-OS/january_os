use crate::mm::boot::setup::MmInitStage;

pub fn current_stage() -> MmInitStage {
    crate::mm::boot::setup::init_stage()
}
