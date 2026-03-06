#[derive(Debug, Clone, Copy)]
pub struct ThreadRegistry;

impl ThreadRegistry {
    pub const fn placeholder() -> Self {
        Self
    }
}
