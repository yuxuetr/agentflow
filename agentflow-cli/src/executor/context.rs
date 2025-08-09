// Execution context management will be implemented here
use std::collections::HashMap;
use serde_json::Value;

pub struct ExecutionContext {
    pub variables: HashMap<String, Value>,
}

impl ExecutionContext {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
        }
    }
}