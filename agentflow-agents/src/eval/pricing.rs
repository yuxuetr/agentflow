//! Per-model pricing table used by the eval harness to translate
//! token usage into a USD cost estimate.
//!
//! Provider crates don't yet report per-call cost (and pricing drifts
//! independently of code), so the harness keeps a separate side-table.
//! Loadable from a YAML file (`pricing.yml`) or constructed in code
//! for tests.
//!
//! Pricing tier model: per-1k tokens for input + output. This mirrors
//! every public provider price sheet (OpenAI / Anthropic / Google /
//! Moonshot / StepFun all quote per-1k or per-1M, which is trivially
//! per-1k after a factor-of-1000 divide).

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// USD price per 1k tokens for one model. `0.0` is a legal value (mock
/// providers, self-hosted models).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelPricing {
  /// USD per 1k input (prompt) tokens.
  #[serde(default)]
  pub input_per_1k: f64,
  /// USD per 1k output (completion) tokens.
  #[serde(default)]
  pub output_per_1k: f64,
}

impl ModelPricing {
  /// Compute the USD cost of one call from prompt + completion token
  /// counts. Token fields are `Option<u32>` because providers may not
  /// report them; missing fields contribute zero to the call's cost
  /// (the alternative would over-report by pretending to know).
  pub fn cost_for_call(self, prompt_tokens: Option<u32>, completion_tokens: Option<u32>) -> f64 {
    let prompt = prompt_tokens.unwrap_or(0) as f64;
    let completion = completion_tokens.unwrap_or(0) as f64;
    (prompt / 1000.0) * self.input_per_1k + (completion / 1000.0) * self.output_per_1k
  }
}

/// Errors surfaced by the pricing table loader.
#[derive(Debug, Error)]
pub enum PricingError {
  #[error("I/O error reading pricing table at {path}: {source}")]
  Io {
    path: String,
    #[source]
    source: std::io::Error,
  },

  #[error("invalid pricing table at {path}: {message}")]
  Parse { path: String, message: String },
}

/// Map of `model_id → ModelPricing`. Use [`PricingTable::lookup`] to
/// fetch a price (returns the table's `default` entry when no exact
/// match is found, falling back to zero cost when no default is set).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PricingTable {
  /// Per-model pricing. Key is the model id as it appears in
  /// `EvalCase::model` / `AgentEvent::LlmCallCompleted::model`.
  #[serde(default)]
  pub models: HashMap<String, ModelPricing>,
  /// Optional fallback when a model id isn't in [`Self::models`].
  /// `None` means "missing models cost $0" — which is the safe default
  /// for the mock provider and self-hosted models.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub default: Option<ModelPricing>,
}

impl PricingTable {
  /// Construct an empty table. Helpful for tests where the eval should
  /// report `cost_usd_actual = 0.0` everywhere.
  pub fn empty() -> Self {
    Self::default()
  }

  /// Load a pricing table from a YAML file. The file shape mirrors
  /// the struct:
  ///
  /// ```yaml
  /// default:
  ///   input_per_1k: 0.0
  ///   output_per_1k: 0.0
  /// models:
  ///   gpt-4o:
  ///     input_per_1k: 5.00
  ///     output_per_1k: 15.00
  /// ```
  pub fn load_from_yaml(path: impl AsRef<Path>) -> Result<Self, PricingError> {
    let p = path.as_ref();
    let body = fs::read_to_string(p).map_err(|e| PricingError::Io {
      path: p.display().to_string(),
      source: e,
    })?;
    let table: PricingTable = serde_yaml::from_str(&body).map_err(|e| PricingError::Parse {
      path: p.display().to_string(),
      message: e.to_string(),
    })?;
    Ok(table)
  }

  /// Resolve the pricing for a model id, falling back to
  /// [`Self::default`] and finally to a zero-cost row.
  pub fn lookup(&self, model: &str) -> ModelPricing {
    if let Some(pricing) = self.models.get(model) {
      return *pricing;
    }
    self.default.unwrap_or_default()
  }

  /// Convenience builder for in-test fixtures.
  pub fn with_model(mut self, model: impl Into<String>, pricing: ModelPricing) -> Self {
    self.models.insert(model.into(), pricing);
    self
  }

  /// Set the fallback pricing for models not enumerated under
  /// [`Self::models`].
  pub fn with_default(mut self, pricing: ModelPricing) -> Self {
    self.default = Some(pricing);
    self
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn model_pricing_cost_for_call_multiplies_per_1k() {
    let p = ModelPricing {
      input_per_1k: 5.0,
      output_per_1k: 15.0,
    };
    // 1k prompt + 1k completion → $5 + $15 = $20.
    let cost = p.cost_for_call(Some(1000), Some(1000));
    assert!((cost - 20.0).abs() < 1e-9, "cost = {cost}");
  }

  #[test]
  fn model_pricing_cost_for_call_handles_partial_token_counts() {
    let p = ModelPricing {
      input_per_1k: 5.0,
      output_per_1k: 15.0,
    };
    // 500 prompt, no completion → $2.5.
    let cost = p.cost_for_call(Some(500), None);
    assert!((cost - 2.5).abs() < 1e-9, "cost = {cost}");
  }

  #[test]
  fn pricing_table_lookup_falls_back_to_default_then_zero() {
    let table = PricingTable::default()
      .with_model(
        "gpt-4o",
        ModelPricing {
          input_per_1k: 5.0,
          output_per_1k: 15.0,
        },
      )
      .with_default(ModelPricing {
        input_per_1k: 0.001,
        output_per_1k: 0.002,
      });
    assert_eq!(table.lookup("gpt-4o").input_per_1k, 5.0);
    assert_eq!(table.lookup("unknown-model").input_per_1k, 0.001);
    // No default set + no models → zero cost.
    let zero_table = PricingTable::default();
    assert_eq!(zero_table.lookup("anything"), ModelPricing::default());
    assert_eq!(
      zero_table
        .lookup("anything")
        .cost_for_call(Some(1_000_000), Some(1_000_000)),
      0.0
    );
  }

  #[test]
  fn pricing_table_load_from_yaml_round_trip() {
    let yaml = r#"
default:
  input_per_1k: 0.001
  output_per_1k: 0.002
models:
  gpt-4o:
    input_per_1k: 5.00
    output_per_1k: 15.00
  claude-sonnet-4-6:
    input_per_1k: 3.00
    output_per_1k: 15.00
"#;
    let f = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(f.path(), yaml).unwrap();
    let table = PricingTable::load_from_yaml(f.path()).unwrap();
    assert_eq!(table.lookup("gpt-4o").input_per_1k, 5.0);
    assert_eq!(table.lookup("claude-sonnet-4-6").output_per_1k, 15.0);
    assert_eq!(table.lookup("missing").input_per_1k, 0.001);
  }

  #[test]
  fn pricing_table_load_from_yaml_rejects_garbage() {
    let f = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(f.path(), "this is: [not valid yaml because:: ::: ").unwrap();
    let err = PricingTable::load_from_yaml(f.path()).unwrap_err();
    assert!(matches!(err, PricingError::Parse { .. }));
  }
}
