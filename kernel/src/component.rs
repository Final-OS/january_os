use alloc::format;
use alloc::string::String;

use crate::config;
use crate::error::{KernelError, KernelResult};
use crate::kprintln;

const MAX_COMPONENTS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ComponentStage {
    Early,
    Core,
    Late,
}

impl ComponentStage {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Early => "early",
            Self::Core => "core",
            Self::Late => "late",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentState {
    Registered,
    Initializing,
    Ready,
    Unsupported,
    Failed,
}

#[derive(Debug, Clone, Copy)]
pub struct ComponentDescriptor {
    pub id: &'static str,
    pub stage: ComponentStage,
    pub deps: &'static [&'static str],
    pub summary: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct ComponentStats {
    pub state: ComponentState,
    pub registrations: u32,
    pub init_calls: u32,
    pub failures: u32,
}

impl ComponentStats {
    pub const fn registered() -> Self {
        Self {
            state: ComponentState::Registered,
            registrations: 1,
            init_calls: 0,
            failures: 0,
        }
    }

    pub const fn ready() -> Self {
        Self {
            state: ComponentState::Ready,
            registrations: 1,
            init_calls: 1,
            failures: 0,
        }
    }

    pub const fn unsupported() -> Self {
        Self {
            state: ComponentState::Unsupported,
            registrations: 1,
            init_calls: 0,
            failures: 0,
        }
    }
}

pub trait KernelComponent {
    fn descriptor(&self) -> ComponentDescriptor;

    fn init_early(&mut self) -> KernelResult<()> {
        Err(KernelError::NotSupported)
    }

    fn init_core(&mut self) -> KernelResult<()> {
        Err(KernelError::NotSupported)
    }

    fn init_late(&mut self) -> KernelResult<()> {
        Err(KernelError::NotSupported)
    }

    fn stats(&self) -> ComponentStats {
        ComponentStats::registered()
    }

    fn dump_state(&self) -> String {
        let descriptor = self.descriptor();
        format!(
            "component={} stage={:?} state={:?}",
            descriptor.id,
            descriptor.stage,
            self.stats().state
        )
    }
}

pub struct ComponentRegistry {
    ready: [Option<&'static str>; MAX_COMPONENTS],
    count: usize,
    highest_stage: ComponentStage,
}

impl ComponentRegistry {
    pub const fn new() -> Self {
        Self {
            ready: [None; MAX_COMPONENTS],
            count: 0,
            highest_stage: ComponentStage::Early,
        }
    }

    pub fn is_ready(&self, id: &str) -> bool {
        self.ready[..self.count].iter().flatten().any(|entry| *entry == id)
    }

    fn validate(&self, component: &ComponentDescriptor) -> Result<(), &'static str> {
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

    fn mark_ready(&mut self, component: &ComponentDescriptor) {
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

pub fn run_component<T>(
    registry: &mut ComponentRegistry,
    component: &'static ComponentDescriptor,
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

pub fn unsupported_component_dump(descriptor: &ComponentDescriptor) -> String {
    format!(
        "component={} stage={:?} status=unsupported deps={:?}",
        descriptor.id, descriptor.stage, descriptor.deps
    )
}
