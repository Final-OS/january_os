use crate::task;

#[inline]
pub fn exit_current_task(exit_code: i32) {
    task::manager::exit_current_task(exit_code);
}

#[inline]
pub fn exit_current_process(exit_code: i32) {
    task::manager::exit_current_process(exit_code);
}
