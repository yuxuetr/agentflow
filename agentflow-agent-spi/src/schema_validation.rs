//! Shared JSON-Schema validation helper (V2.1).
//!
//! Extracted from [`crate::delegation::validate_output`] (L5.1) so
//! `agentflow-agents`' `ReActAgent`/`PlanExecuteAgent` `output_schema`
//! support (V2.1) can validate a candidate final answer against a caller
//! -supplied schema without duplicating the `jsonschema` compile/validate/
//! collect-error-strings boilerplate — the same pattern independently
//! reimplemented a third time in `agentflow-tool::ToolRegistry::
//! validate_params` (Q2.9.3) for tool-call arguments.

use serde_json::Value;

/// Parse `text` as JSON and validate it against `schema`.
///
/// Returns `Ok(())` when `text` parses as JSON and validates; otherwise
/// `Err` with one message per failure — `text` not being valid JSON,
/// `schema` not being valid JSON Schema, or one message per schema
/// violation reported by the `jsonschema` crate.
pub fn validate_json_str_against_schema(schema: &Value, text: &str) -> Result<(), Vec<String>> {
  let parsed: Value =
    serde_json::from_str(text).map_err(|e| vec![format!("output is not valid JSON: {e}")])?;
  validate_json_against_schema(schema, &parsed)
}

/// Validate an already-parsed [`Value`] against `schema`. See
/// [`validate_json_str_against_schema`] for the text-parsing variant.
pub fn validate_json_against_schema(schema: &Value, value: &Value) -> Result<(), Vec<String>> {
  let compiled = jsonschema::JSONSchema::options()
    .compile(schema)
    .map_err(|e| vec![format!("schema is not valid JSON Schema: {e}")])?;
  match compiled.validate(value) {
    Ok(()) => Ok(()),
    Err(errors) => Err(errors.map(|e| e.to_string()).collect()),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  fn answer_schema() -> Value {
    json!({
      "type": "object",
      "properties": {"answer": {"type": "string"}},
      "required": ["answer"]
    })
  }

  #[test]
  fn valid_json_matching_schema_passes() {
    assert!(validate_json_str_against_schema(&answer_schema(), r#"{"answer":"42"}"#).is_ok());
  }

  #[test]
  fn non_json_text_fails_with_parse_error() {
    let errors =
      validate_json_str_against_schema(&answer_schema(), "not json").expect_err("must fail");
    assert!(errors.iter().any(|e| e.contains("not valid JSON")));
  }

  #[test]
  fn json_that_violates_schema_fails_with_schema_errors() {
    let errors = validate_json_str_against_schema(&answer_schema(), r#"{"wrong_field": 1}"#)
      .expect_err("must fail");
    assert!(!errors.is_empty());
  }

  #[test]
  fn invalid_schema_itself_fails_with_a_clear_message() {
    let bad_schema = json!({"type": "not-a-real-type"});
    let errors =
      validate_json_str_against_schema(&bad_schema, r#"{"answer":"42"}"#).expect_err("must fail");
    assert!(errors.iter().any(|e| e.contains("not valid JSON Schema")));
  }

  #[test]
  fn validate_json_against_schema_skips_the_text_parse_step() {
    assert!(validate_json_against_schema(&answer_schema(), &json!({"answer": "hi"})).is_ok());
    assert!(validate_json_against_schema(&answer_schema(), &json!({})).is_err());
  }
}
