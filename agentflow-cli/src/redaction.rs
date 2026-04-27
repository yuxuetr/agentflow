use anyhow::Result;
use serde::Serialize;
use serde_json::Value;

use agentflow_tracing::{redact_text, redact_value, RedactionConfig};

pub fn redact_cli_text(value: impl AsRef<str>) -> String {
  redact_text(value.as_ref(), &RedactionConfig::default())
}

pub fn redact_cli_value(value: &mut Value) {
  redact_value(value, &RedactionConfig::default());
}

pub fn to_redacted_json_value(value: impl Serialize) -> Result<Value> {
  let mut value = serde_json::to_value(value)?;
  redact_cli_value(&mut value);
  Ok(value)
}
