use crate::config;
use crate::kprintln;

const MAX_COMPONENTS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum KernelComponentStage {
    Early,
    Core,
    Late,
}

impl KernelComponentStage {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Early => "early",
            Self::Core => "core",
            Self::Late => "late",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct KernelComponentDescriptor {
    pub id: &'static str,
    pub stage: KernelComponentStage,
    pub deps: &'static [&'static str],
    pub summary: &'static str,
}

pub struct KernelComponentRegistry {
    ready: [Option<&'static str>; MAX_COMPONENTS],
    count: usize,
    highest_stage: KernelComponentStage,
}

impl KernelComponentRegistry {
    pub const fn new() -> Self {
        Self {
            ready: [None; MAX_COMPONENTS],
            count: 0,
            highest_stage: KernelComponentStage::Early,
        }
    }

    pub fn is_ready(&self, id: &str) -> bool {
        self.ready[..self.count].iter().flatten().any(|entry| *entry == id)
    }

    fn validate(&self, component: &KernelComponentDescriptor) -> Result<(), &'static str> {
        if component.stage < self.highest_stage {
            return Err("component stage regressed");
        }
        for dep in component.deps.iter() {
            if !self.is_ready(dep) {
                return Err("component dependency not ready");
            }
        }
        Ok(())
    }

    fn mark_ready(&mut self, component: &KernelComponentDescriptor) {
        if self.is_ready(component.id) {
            return;
        }
        assert!(self.count < MAX_COMPONENTS, "too many kernel components");
        self.ready[self.count] = Some(component.id);
        self.count += 1;
        if component.stage > self.highest_stage {
            self.highest_stage = component.stage;
        }
    }
}

pub fn run_kernel_component<T>(
    registry: &mut KernelComponentRegistry,
    component: &'static KernelComponentDescriptor,
    init: impl FnOnce() -> T,
) -> T {
    if let Err(reason) = registry.validate(component) {
        panic!(
            "component init rejected: id={} stage={} reason={} deps={:?}",
            component.id,
            component.stage.as_str(),
            reason,
            component.deps,
        );
    }

    if config::DEBUG_VERBOSE {
        kprintln!(
            "\x1b[90m[diag]\x1b[0m[component] enter id={} stage={} deps={:?} summary={}",
            component.id,
            component.stage.as_str(),
            component.deps,
            component.summary,
        );
    }

    let output = init();
    registry.mark_ready(component);

    if config::DEBUG_VERBOSE {
        kprintln!(
            "\x1b[90m[diag]\x1b[0m[component] ready id={} stage={}",
            component.id,
            component.stage.as_str(),
        );
    }

    output
}
