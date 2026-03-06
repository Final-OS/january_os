use crate::virt::error::VirtResult;

pub fn dispatch(nr: usize, arg0: usize, arg1: usize) -> VirtResult<usize> {
    super::handlers::handle(nr, arg0, arg1)
}
