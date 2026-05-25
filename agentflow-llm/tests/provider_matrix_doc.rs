//! Doc / type drift detection for `docs/LLM_PROVIDERS_MATRIX.md`.
//!
//! Adding or renaming a field on [`ProviderRequest`] or a variant on
//! [`ToolChoice`] must come with a documentation update. These tests
//! enforce that contract by:
//!
//! 1. Destructuring the type exhaustively at compile time, so a
//!    forgotten field fails the build (not just the test).
//! 2. Asserting the expected names appear verbatim in the matrix
//!    document, so silent renames also fail.
//!
//! Verified in Phase H3 prereq work (P3.7).
//!
//! See [`docs/LLM_PROVIDERS_MATRIX.md`] for the human-readable
//! contract.

use std::path::PathBuf;

use agentflow_llm::providers::ProviderRequest;
use agentflow_llm::tool_calling::ToolChoice;

const MATRIX_DOC_RELATIVE: &str = "../docs/LLM_PROVIDERS_MATRIX.md";

fn matrix_doc() -> String {
  let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  path.push(MATRIX_DOC_RELATIVE);
  std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

/// Compile-time check: forces every test below to pick up new
/// [`ProviderRequest`] fields. Destructuring a struct with unhandled
/// fields is a build error, not a warning, so the type's authoritative
/// field set is whatever this function names.
fn provider_request_field_names() -> [&'static str; 7] {
  // Type-system check: if a field is added or renamed on
  // ProviderRequest, the destructure stops compiling and CI fails
  // before the runtime assertion below ever runs. The closure is
  // never invoked.
  let _ensure_fields = |req: ProviderRequest| {
    let ProviderRequest {
      model: _,
      messages: _,
      stream: _,
      parameters: _,
      tools: _,
      tool_choice: _,
      thinking: _,
    } = req;
  };
  // Keep alphabetical so the test failure message is stable.
  [
    "messages",
    "model",
    "parameters",
    "stream",
    "thinking",
    "tool_choice",
    "tools",
  ]
}

fn tool_choice_variants() -> [&'static str; 4] {
  // Compile-time exhaustion check.
  let _ = |c: ToolChoice| match c {
    ToolChoice::Auto => {}
    ToolChoice::None => {}
    ToolChoice::Required => {}
    ToolChoice::Tool { .. } => {}
  };
  ["auto", "none", "required", "tool"]
}

#[test]
fn every_provider_request_field_is_documented_in_matrix() {
  let doc = matrix_doc();
  let mut missing = Vec::new();
  for field in provider_request_field_names() {
    // Search for the field name wrapped in backticks so we match
    // documented references rather than incidental occurrences.
    let needle = format!("`{field}`");
    if !doc.contains(&needle) {
      missing.push(field);
    }
  }
  assert!(
    missing.is_empty(),
    "ProviderRequest fields not documented in docs/LLM_PROVIDERS_MATRIX.md: {missing:?}\n\
     Update the `ProviderRequest contract` section so each field appears as `<name>`."
  );
}

#[test]
fn every_tool_choice_variant_is_documented_in_matrix() {
  let doc = matrix_doc();
  let mut missing = Vec::new();
  for variant in tool_choice_variants() {
    let needle = format!("`{variant}`");
    if !doc.contains(&needle) {
      missing.push(variant);
    }
  }
  assert!(
    missing.is_empty(),
    "ToolChoice variants not documented in docs/LLM_PROVIDERS_MATRIX.md: {missing:?}\n\
     Update the `ToolChoice modes` section."
  );
}

#[test]
fn matrix_doc_references_core_model_capability_flags() {
  let doc = matrix_doc();
  // ModelCapabilities flags that drive provider validation /
  // ReAct fallback. These names must appear in the doc so callers
  // know the runtime levers.
  let required_flags = [
    "supports_streaming",
    "requires_streaming",
    "supports_tools",
    "native_tool_calling",
    "max_context_tokens",
    "max_output_tokens",
    "supports_system_messages",
  ];
  let mut missing = Vec::new();
  for flag in required_flags {
    let needle = format!("`{flag}`");
    if !doc.contains(&needle) {
      missing.push(flag);
    }
  }
  assert!(
    missing.is_empty(),
    "ModelCapabilities flags not documented in docs/LLM_PROVIDERS_MATRIX.md: {missing:?}"
  );
}

#[test]
fn matrix_doc_keeps_verification_status_vocabulary() {
  let doc = matrix_doc();
  let required = [
    "`tested`",
    "`best_effort`",
    "`live_tested`",
    "`mock_only`",
    "`unsupported`",
  ];
  for needle in required {
    assert!(
      doc.contains(needle),
      "matrix doc missing verification status `{needle}`"
    );
  }
}
