//! `agentflow rag eval` — run the RAG eval harness over a dataset directory.
//!
//! The CLI is deliberately scoped to the offline BM25 retriever for the v0.4.0
//! milestone: it requires no external services, runs on every CI host, and
//! produces a deterministic report. Vector / hybrid retrievers can plug in
//! later via additional `--retriever` values.

use agentflow_rag::eval::{
  Bm25Eval, ComparisonReport, Dataset, EvalConfig, EvalReport, compare, evaluate,
};
use anyhow::{Context, Result, bail};
use colored::Colorize;
use serde_json::json;
use std::path::PathBuf;

/// Execute the `rag eval` command.
///
/// `k_values` defaults to `[1, 3, 5, 10]` when empty. `retriever` controls the
/// backend (currently only `"bm25"`). When `compare_to` is `Some`, the CLI runs
/// a second BM25 eval with custom params parsed from the form `k1=1.5,b=0.6`
/// and emits a paired comparison report.
pub async fn execute(
  dataset_dir: PathBuf,
  retriever: String,
  k_values: Vec<usize>,
  compare_to: Option<String>,
  output: Option<PathBuf>,
) -> Result<()> {
  let dataset = Dataset::load_from_dir(&dataset_dir)
    .with_context(|| format!("loading dataset from {}", dataset_dir.display()))?;

  println!(
    "{}",
    format!("Loaded dataset: {}", dataset_dir.display())
      .bold()
      .blue()
  );
  if let Some(name) = &dataset.manifest.name {
    println!("  manifest: name={}", name);
  }
  println!(
    "  corpus={} queries={} judgments={}",
    dataset.corpus.len(),
    dataset.queries.len(),
    dataset.judgments.len()
  );

  let k_values = if k_values.is_empty() {
    vec![1usize, 3, 5, 10]
  } else {
    k_values
  };

  let baseline_report = run_eval(&dataset, &retriever, &k_values, "baseline")?;
  println!();
  println!("{}", baseline_report.render_table());

  let mut comparison: Option<ComparisonReport> = None;
  let mut candidate_report: Option<EvalReport> = None;
  if let Some(spec) = &compare_to {
    let candidate = run_compare_candidate(&dataset, spec, &k_values)?;
    println!();
    println!("{}", candidate.render_table());
    let cmp = compare(&baseline_report, &candidate);
    println!();
    println!("{}", cmp.render_table().bold());
    candidate_report = Some(candidate);
    comparison = Some(cmp);
  }

  if let Some(path) = output {
    let payload = json!({
      "dataset": {
        "path": dataset_dir.display().to_string(),
        "manifest": {
          "name": dataset.manifest.name,
          "version": dataset.manifest.version,
          "source": dataset.manifest.source,
          "license": dataset.manifest.license,
        },
        "corpus_size": dataset.corpus.len(),
        "queries": dataset.queries.len(),
        "judgments": dataset.judgments.len(),
      },
      "baseline": baseline_report,
      "candidate": candidate_report,
      "comparison": comparison,
    });
    std::fs::write(&path, serde_json::to_string_pretty(&payload)?)
      .with_context(|| format!("writing report to {}", path.display()))?;
    println!(
      "{}",
      format!("Report written to {}", path.display()).green()
    );
  }

  Ok(())
}

fn run_eval(
  dataset: &Dataset,
  retriever_kind: &str,
  k_values: &[usize],
  label: &str,
) -> Result<EvalReport> {
  let config = EvalConfig {
    k_values: k_values.to_vec(),
    label: label.to_string(),
  };
  match retriever_kind {
    "bm25" => {
      let retriever = Bm25Eval::from_dataset(dataset);
      evaluate(&retriever, dataset, &config).map_err(anyhow::Error::from)
    }
    other => bail!(
      "unsupported retriever `{}`. Supported: bm25 (vector/hybrid pending)",
      other
    ),
  }
}

fn run_compare_candidate(dataset: &Dataset, spec: &str, k_values: &[usize]) -> Result<EvalReport> {
  let (k1, b) = parse_bm25_params(spec)?;
  let config = EvalConfig {
    k_values: k_values.to_vec(),
    label: format!("bm25(k1={},b={})", k1, b),
  };
  let retriever = Bm25Eval::with_params(dataset, k1, b);
  evaluate(&retriever, dataset, &config).map_err(anyhow::Error::from)
}

/// Parse `--compare-to "k1=1.5,b=0.6"` into `(k1, b)`. Both keys are required;
/// missing or unrecognized keys produce a clear error rather than silently
/// defaulting.
fn parse_bm25_params(spec: &str) -> Result<(f32, f32)> {
  let mut k1: Option<f32> = None;
  let mut b: Option<f32> = None;
  for part in spec.split(',') {
    let part = part.trim();
    if part.is_empty() {
      continue;
    }
    let (key, value) = part
      .split_once('=')
      .with_context(|| format!("expected key=value, got `{}`", part))?;
    let key = key.trim();
    let value: f32 = value
      .trim()
      .parse()
      .with_context(|| format!("invalid number for `{}`: `{}`", key, value))?;
    match key {
      "k1" => k1 = Some(value),
      "b" => b = Some(value),
      other => bail!("unknown BM25 param `{}` (expected k1 or b)", other),
    }
  }
  match (k1, b) {
    (Some(k1), Some(b)) => Ok((k1, b)),
    _ => bail!("--compare-to expects both k1 and b, e.g. \"k1=1.5,b=0.6\""),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parse_bm25_params_basic() {
    let (k1, b) = parse_bm25_params("k1=1.8,b=0.6").unwrap();
    assert!((k1 - 1.8).abs() < 1e-6);
    assert!((b - 0.6).abs() < 1e-6);
  }

  #[test]
  fn parse_bm25_params_rejects_missing_key() {
    assert!(parse_bm25_params("k1=1.5").is_err());
  }

  #[test]
  fn parse_bm25_params_rejects_unknown_key() {
    assert!(parse_bm25_params("k1=1.5,b=0.5,xyz=1").is_err());
  }

  #[test]
  fn parse_bm25_params_rejects_non_numeric() {
    assert!(parse_bm25_params("k1=oops,b=0.5").is_err());
  }
}
