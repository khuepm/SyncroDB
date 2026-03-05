use crate::error::Result;

pub struct MigrationGenerator;

impl MigrationGenerator {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MigrationGenerator {
    fn default() -> Self {
        Self::new()
    }
}
