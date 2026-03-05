use crate::error::Result;

pub struct DiffEngine;

impl DiffEngine {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DiffEngine {
    fn default() -> Self {
        Self::new()
    }
}
