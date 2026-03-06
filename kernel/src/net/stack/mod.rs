pub mod egress;
pub mod ingress;
pub mod neighbor;
pub mod proto;
pub mod route;
pub mod service;

pub fn init_stack() -> crate::net::error::NetResult<()> {
    service::bring_up_default_stack()
}
