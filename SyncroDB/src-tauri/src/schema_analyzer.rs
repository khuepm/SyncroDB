use crate::error::Result;

pub struct SchemaAnalyzer;

impl SchemaAnalyzer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SchemaAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}
