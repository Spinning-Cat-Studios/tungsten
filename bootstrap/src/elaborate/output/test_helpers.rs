use super::ElabOutput;

impl ElabOutput {
    /// Minimal empty output for tests. All collections empty.
    pub fn test_empty() -> Self {
        Self::default()
    }
}
