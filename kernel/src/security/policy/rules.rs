#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyRuleDomain {
    File,
    Net,
    Task,
    Capability,
    Audit,
}

#[derive(Debug, Clone, Copy)]
pub struct PolicyRuleSet {
    pub domain: PolicyRuleDomain,
    pub rule_count: u32,
    pub loaded: bool,
}

impl PolicyRuleSet {
    pub const fn placeholder(domain: PolicyRuleDomain) -> Self {
        Self {
            domain,
            rule_count: 0,
            loaded: false,
        }
    }
}
