#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmLifecycleState {
    Created,
    Running,
    Paused,
    Stopped,
    Destroyed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VcpuLifecycleState {
    Created,
    Ready,
    Running,
    Paused,
    Stopped,
}

#[derive(Debug, Clone, Copy)]
pub struct VirtState {
    pub detection_ready: bool,
    pub vm_ready: bool,
    pub vcpu_ready: bool,
    pub memory_ready: bool,
    pub irq_ready: bool,
    pub device_ready: bool,
}

impl VirtState {
    pub const fn host_placeholder() -> Self {
        Self {
            detection_ready: true,
            vm_ready: false,
            vcpu_ready: false,
            memory_ready: false,
            irq_ready: false,
            device_ready: false,
        }
    }
}
