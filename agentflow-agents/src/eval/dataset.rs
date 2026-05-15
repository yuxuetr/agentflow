//! On-disk eval dataset: `dataset.toml` manifest + `cases.jsonl` body.
//!
//! See `docs/AGENT_EVAL_FORMAT.md` for the operator-facing description.

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use super::assertion::Assertion;

/// Errors surfaced by the eval dataset loader.
#[derive(Debug, Error)]
pub enum EvalError {
  /// I/O failure while reading the dataset directory.
  #[error("I/O error reading {path}: {source}")]
  Io {
    path: String,
    #[source]
    source: std::io::Error,
  },

  /// Malformed `dataset.toml`.
  #[error("invalid dataset manifest at {path}: {message}")]
  ManifestParse { path: String, message: String },

  /// Malformed JSONL row.
  #[error("invalid cases.jsonl line {line} in {path}: {message}")]
  CaseParse {
    path: String,
    line: usize,
    message: String,
  },

  /// Validation rejected the dataset (e.g. duplicate ids, empty assertions).
  #[error("dataset validation failed: {message}")]
  Validation { message: String },
}

/// Per-case default values declared in `dataset.toml [defaults]`. Each
/// `EvalCase` inherits any of these fields it does not set explicitly.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EvalCaseDefaults {
  /// Default skill path / registry name.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub skill: Option<String>,
  /// Default LLM model id.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub model: Option<String>,
  /// Maps to `RuntimeLimits::max_steps`.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub max_steps: Option<usize>,
  /// Maps to `RuntimeLimits::max_tool_calls`.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub max_tool_calls: Option<usize>,
  /// Hard cap on accumulated provider cost. Run fails with
  /// `AgentStopReason::CostLimitExceeded` when crossed.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub cost_limit_usd: Option<f64>,
  /// Maps to `RuntimeLimits::timeout_ms`.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub latency_limit_ms: Option<u64>,
  /// Maps to `RuntimeLimits::token_budget`.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub token_budget: Option<u32>,
}

/// Top-level `dataset.toml` manifest. The `[defaults]` table is optional;
/// when omitted every case must specify the fields it cares about
/// explicitly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetManifest {
  /// Format version. Increments only on incompatible changes.
  #[serde(default = "default_schema_version")]
  pub schema_version: u32,
  pub name: String,
  pub version: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub description: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub source: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub license: Option<String>,
  #[serde(default)]
  pub defaults: EvalCaseDefaults,
}

fn default_schema_version() -> u32 {
  1
}

/// Raw form of one JSONL line. Fields are all `Option<_>` so the loader can
/// detect "not specified" vs "explicitly zero" and apply defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawEvalCase {
  pub id: String,
  pub prompt: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub skill: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub model: Option<String>,
  #[serde(default)]
  pub tools_allowed: Vec<String>,
  #[serde(default)]
  pub tools_denied: Vec<String>,
  /// Top-level structured inputs injected into the agent's initial state.
  #[serde(default)]
  pub inputs: BTreeMap<String, Value>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub max_steps: Option<usize>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub max_tool_calls: Option<usize>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub cost_limit_usd: Option<f64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub latency_limit_ms: Option<u64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub token_budget: Option<u32>,
  #[serde(default)]
  pub expected_assertions: Vec<Assertion>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub notes: Option<String>,
}

/// One fully-resolved eval case: defaults from the manifest applied, ids
/// validated as unique, assertion list non-empty.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalCase {
  pub id: String,
  pub prompt: String,
  pub skill: Option<String>,
  pub model: Option<String>,
  pub tools_allowed: Vec<String>,
  pub tools_denied: Vec<String>,
  pub inputs: BTreeMap<String, Value>,
  pub max_steps: Option<usize>,
  pub max_tool_calls: Option<usize>,
  pub cost_limit_usd: Option<f64>,
  pub latency_limit_ms: Option<u64>,
  pub token_budget: Option<u32>,
  pub expected_assertions: Vec<Assertion>,
  pub notes: Option<String>,
}

impl EvalCase {
  /// Merge a [`RawEvalCase`] with the dataset defaults to produce a fully-
  /// resolved case. Returns a validation error when the merged result has
  /// no assertions or a missing `skill` / `model`.
  pub fn from_raw(raw: RawEvalCase, defaults: &EvalCaseDefaults) -> Result<Self, EvalError> {
    if raw.id.trim().is_empty() {
      return Err(EvalError::Validation {
        message: "case id must not be empty".to_string(),
      });
    }
    if raw.prompt.trim().is_empty() {
      return Err(EvalError::Validation {
        message: format!("case '{}' has empty prompt", raw.id),
      });
    }
    if raw.expected_assertions.is_empty() {
      return Err(EvalError::Validation {
        message: format!(
          "case '{}' has no expected_assertions — always-pass cases are a mistake",
          raw.id
        ),
      });
    }
    Ok(Self {
      skill: raw.skill.or_else(|| defaults.skill.clone()),
      model: raw.model.or_else(|| defaults.model.clone()),
      max_steps: raw.max_steps.or(defaults.max_steps),
      max_tool_calls: raw.max_tool_calls.or(defaults.max_tool_calls),
      cost_limit_usd: raw.cost_limit_usd.or(defaults.cost_limit_usd),
      latency_limit_ms: raw.latency_limit_ms.or(defaults.latency_limit_ms),
      token_budget: raw.token_budget.or(defaults.token_budget),
      id: raw.id,
      prompt: raw.prompt,
      tools_allowed: raw.tools_allowed,
      tools_denied: raw.tools_denied,
      inputs: raw.inputs,
      expected_assertions: raw.expected_assertions,
      notes: raw.notes,
    })
  }
}

/// One fully-loaded dataset: manifest plus resolved cases. Use
/// [`Dataset::load_from_dir`] to read from disk; construct directly for
/// tests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dataset {
  pub manifest: DatasetManifest,
  /// Absolute path the dataset was loaded from. Resolved fixture paths in
  /// `EvalCase::inputs` are interpreted relative to this directory.
  pub root: PathBuf,
  pub cases: Vec<EvalCase>,
}

impl Dataset {
  /// Load a dataset directory (`dataset.toml` + `cases.jsonl`).
  pub fn load_from_dir(dir: impl AsRef<Path>) -> Result<Self, EvalError> {
    let root = dir.as_ref().to_path_buf();
    let manifest_path = root.join("dataset.toml");
    let manifest_raw = fs::read_to_string(&manifest_path).map_err(|e| EvalError::Io {
      path: manifest_path.display().to_string(),
      source: e,
    })?;
    let manifest: DatasetManifest =
      toml::from_str(&manifest_raw).map_err(|e| EvalError::ManifestParse {
        path: manifest_path.display().to_string(),
        message: e.to_string(),
      })?;

    let cases_path = root.join("cases.jsonl");
    let file = fs::File::open(&cases_path).map_err(|e| EvalError::Io {
      path: cases_path.display().to_string(),
      source: e,
    })?;
    let mut cases: Vec<EvalCase> = Vec::new();
    let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (idx, line) in BufReader::new(file).lines().enumerate() {
      let line_no = idx + 1;
      let line = line.map_err(|e| EvalError::Io {
        path: cases_path.display().to_string(),
        source: e,
      })?;
      let trimmed = line.trim();
      if trimmed.is_empty() || trimmed.starts_with("//") {
        continue;
      }
      let raw: RawEvalCase = serde_json::from_str(trimmed).map_err(|e| EvalError::CaseParse {
        path: cases_path.display().to_string(),
        line: line_no,
        message: e.to_string(),
      })?;
      let case = EvalCase::from_raw(raw, &manifest.defaults)?;
      if !seen_ids.insert(case.id.clone()) {
        return Err(EvalError::Validation {
          message: format!(
            "duplicate case id '{}' at line {} in {}",
            case.id,
            line_no,
            cases_path.display()
          ),
        });
      }
      cases.push(case);
    }
    if cases.is_empty() {
      return Err(EvalError::Validation {
        message: format!(
          "no cases parsed from {} — dataset must contain at least one EvalCase",
          cases_path.display()
        ),
      });
    }
    Ok(Self {
      manifest,
      root,
      cases,
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;
  use std::io::Write;

  fn write_dataset(
    root: &Path,
    manifest_toml: &str,
    cases_jsonl: &[&str],
  ) -> Result<(), std::io::Error> {
    fs::write(root.join("dataset.toml"), manifest_toml)?;
    let mut f = fs::File::create(root.join("cases.jsonl"))?;
    for line in cases_jsonl {
      writeln!(f, "{line}")?;
    }
    Ok(())
  }

  #[test]
  fn load_from_dir_applies_defaults_to_each_case() {
    let dir = tempfile::tempdir().unwrap();
    write_dataset(
      dir.path(),
      r#"
schema_version = 1
name = "fixture"
version = "0.0.1"

[defaults]
skill = "default-skill"
model = "mock-model"
max_steps = 8
"#,
      &[
        r#"{"id":"a","prompt":"first","expected_assertions":[{"type":"contains","needle":"hi"}]}"#,
        r#"{"id":"b","prompt":"second","max_steps":12,"expected_assertions":[{"type":"contains","needle":"yo"}]}"#,
      ],
    )
    .unwrap();
    let ds = Dataset::load_from_dir(dir.path()).unwrap();
    assert_eq!(ds.cases.len(), 2);
    assert_eq!(ds.cases[0].skill.as_deref(), Some("default-skill"));
    assert_eq!(ds.cases[0].model.as_deref(), Some("mock-model"));
    assert_eq!(ds.cases[0].max_steps, Some(8));
    // Per-case override beats the default.
    assert_eq!(ds.cases[1].max_steps, Some(12));
  }

  #[test]
  fn load_from_dir_rejects_duplicate_case_ids() {
    let dir = tempfile::tempdir().unwrap();
    write_dataset(
      dir.path(),
      r#"
schema_version = 1
name = "fixture"
version = "0.0.1"
"#,
      &[
        r#"{"id":"dup","prompt":"x","expected_assertions":[{"type":"contains","needle":"y"}]}"#,
        r#"{"id":"dup","prompt":"x2","expected_assertions":[{"type":"contains","needle":"z"}]}"#,
      ],
    )
    .unwrap();
    let err = Dataset::load_from_dir(dir.path()).unwrap_err();
    assert!(matches!(err, EvalError::Validation { .. }));
  }

  #[test]
  fn load_from_dir_rejects_empty_assertions_list() {
    let dir = tempfile::tempdir().unwrap();
    write_dataset(
      dir.path(),
      r#"
schema_version = 1
name = "fixture"
version = "0.0.1"
"#,
      &[r#"{"id":"a","prompt":"x","expected_assertions":[]}"#],
    )
    .unwrap();
    let err = Dataset::load_from_dir(dir.path()).unwrap_err();
    assert!(matches!(err, EvalError::Validation { .. }));
  }

  #[test]
  fn load_from_dir_rejects_empty_dataset() {
    let dir = tempfile::tempdir().unwrap();
    write_dataset(
      dir.path(),
      r#"
schema_version = 1
name = "fixture"
version = "0.0.1"
"#,
      &[],
    )
    .unwrap();
    let err = Dataset::load_from_dir(dir.path()).unwrap_err();
    assert!(matches!(err, EvalError::Validation { .. }));
  }

  #[test]
  fn raw_case_round_trips_through_json() {
    let raw = RawEvalCase {
      id: "case-1".to_string(),
      prompt: "hi".to_string(),
      skill: Some("rust_expert".to_string()),
      model: Some("mock-model".to_string()),
      tools_allowed: vec!["file".to_string()],
      tools_denied: vec!["shell".to_string()],
      inputs: BTreeMap::from([("k".to_string(), json!(1))]),
      max_steps: Some(8),
      max_tool_calls: Some(4),
      cost_limit_usd: Some(0.5),
      latency_limit_ms: Some(30_000),
      token_budget: Some(50_000),
      expected_assertions: vec![Assertion::Contains {
        needle: "hi".to_string(),
        target: AssertionTarget::FinalAnswer,
        case_insensitive: false,
      }],
      notes: Some("hello".to_string()),
    };
    let serialized = serde_json::to_string(&raw).unwrap();
    let parsed: RawEvalCase = serde_json::from_str(&serialized).unwrap();
    assert_eq!(parsed.id, "case-1");
    assert_eq!(parsed.tools_allowed, vec!["file".to_string()]);
    assert_eq!(parsed.inputs.get("k").cloned(), Some(json!(1)));
  }

  // Pull in the assertion target enum needed for round-trip test.
  use super::super::assertion::AssertionTarget;
}
