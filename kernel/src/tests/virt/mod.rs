pub mod detect;
pub mod hypercall;
pub mod irq;
pub mod memory;
pub mod recovery;
pub mod vcpu;
pub mod vm;

pub fn run() {
    detect::run();
    vm::run();
    vcpu::run();
    memory::run();
    irq::run();
    hypercall::run();
    recovery::run();
}
