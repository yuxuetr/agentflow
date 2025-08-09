// Workflow configuration structures will be implemented here
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct WorkflowConfig {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
}

// More structures will be added as we implement the workflow system