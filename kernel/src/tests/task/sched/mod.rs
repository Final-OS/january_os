pub mod context_switch;

pub(super) use super::task_step;

pub(super) fn run_context_switch() {
    context_switch::run();
}
