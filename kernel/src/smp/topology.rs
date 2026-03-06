#[derive(Debug, Clone, Copy)]
pub struct TopologySnapshot {
    pub detected_cpus: usize,
    pub online_cpus: usize,
}
