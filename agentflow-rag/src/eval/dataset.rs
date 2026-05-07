//! Dataset format for the RAG eval harness.
//!
//! A dataset is a triple `(corpus, queries, judgments)`:
//!
//! - `corpus`: id-keyed text documents
//! - `queries`: id-keyed natural-language queries
//! - `judgments`: per-query relevance grades for known docs
//!
//! On disk, datasets are JSONL — three files (`corpus.jsonl`,
//! `queries.jsonl`, `qrels.jsonl`) plus an optional `dataset.toml`
//! manifest (name / version / source / license). Loading via
//! [`Dataset::load_from_dir`] is the common path; tests construct
//! datasets in memory with [`Dataset::new`].
//!
//! Relevance scores are integers (`u8`). Binary datasets use `0` /
//! `1`; graded datasets (TREC-style) typically use `0..=3`.

use crate::error::{RAGError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Integer relevance score; 0 means non-relevant.
pub type RelevanceScore = u8;

/// One corpus document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorpusDoc {
  /// Stable document identifier referenced by judgments.
  pub id: String,
  /// Document text body.
  pub text: String,
  /// Optional title (some datasets keep this separate from the body).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub title: Option<String>,
}

/// One query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
  pub id: String,
  pub text: String,
  /// Free-form notes (e.g. annotation rationale, dataset slice tag).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub notes: Option<String>,
}

/// Per-query relevance map. `relevances` keeps both relevant (>0) and
/// optionally explicit non-relevant (0) annotations — only positives count
/// for Recall / MRR / nDCG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Judgment {
  pub query_id: String,
  /// `doc_id → score`. Use `0` to record "explicitly judged non-relevant".
  pub relevances: HashMap<String, RelevanceScore>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub notes: Option<String>,
}

impl Judgment {
  /// Iterate over `(doc_id, score)` pairs with score > 0.
  pub fn relevant_pairs(&self) -> impl Iterator<Item = (&str, RelevanceScore)> {
    self
      .relevances
      .iter()
      .filter(|(_, score)| **score > 0)
      .map(|(id, score)| (id.as_str(), *score))
  }

  /// Iterate over relevant doc ids only (score > 0).
  pub fn relevant_ids(&self) -> impl Iterator<Item = &str> {
    self.relevant_pairs().map(|(id, _)| id)
  }

  /// All non-zero relevance scores. Used by nDCG to compute IDCG.
  pub fn relevances(&self) -> impl Iterator<Item = RelevanceScore> + '_ {
    self.relevances.values().copied().filter(|s| *s > 0)
  }

  /// Score for `doc_id`; 0 if absent or explicitly non-relevant.
  pub fn relevance(&self, doc_id: &str) -> RelevanceScore {
    self.relevances.get(doc_id).copied().unwrap_or(0)
  }

  pub fn is_relevant(&self, doc_id: &str) -> bool {
    self.relevance(doc_id) > 0
  }
}

/// Dataset manifest (`dataset.toml`). Optional metadata for provenance.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DatasetManifest {
  #[serde(default)]
  pub name: Option<String>,
  #[serde(default)]
  pub version: Option<String>,
  #[serde(default)]
  pub source: Option<String>,
  #[serde(default)]
  pub license: Option<String>,
  #[serde(default)]
  pub description: Option<String>,
}

/// In-memory evaluation dataset.
#[derive(Debug, Clone)]
pub struct Dataset {
  pub manifest: DatasetManifest,
  pub corpus: Vec<CorpusDoc>,
  pub queries: Vec<Query>,
  pub judgments: Vec<Judgment>,
}

impl Dataset {
  pub fn new(corpus: Vec<CorpusDoc>, queries: Vec<Query>, judgments: Vec<Judgment>) -> Self {
    Self {
      manifest: DatasetManifest::default(),
      corpus,
      queries,
      judgments,
    }
  }

  pub fn with_manifest(mut self, manifest: DatasetManifest) -> Self {
    self.manifest = manifest;
    self
  }

  /// Look up a query's judgment by id. O(n); judgment counts in this harness
  /// are typically small (a few hundred). Build a HashMap if you need more.
  pub fn judgment_for(&self, query_id: &str) -> Option<&Judgment> {
    self.judgments.iter().find(|j| j.query_id == query_id)
  }

  /// Sanity-check the dataset: every judgment references a known query, and
  /// every relevance refers to a known corpus doc. Returns an error listing
  /// the first few violations rather than panicking.
  pub fn validate(&self) -> Result<()> {
    let known_queries: std::collections::HashSet<&str> =
      self.queries.iter().map(|q| q.id.as_str()).collect();
    let known_docs: std::collections::HashSet<&str> =
      self.corpus.iter().map(|d| d.id.as_str()).collect();

    let mut errors: Vec<String> = Vec::new();
    for judgment in &self.judgments {
      if !known_queries.contains(judgment.query_id.as_str()) {
        errors.push(format!(
          "judgment references unknown query_id `{}`",
          judgment.query_id
        ));
      }
      for doc_id in judgment.relevances.keys() {
        if !known_docs.contains(doc_id.as_str()) {
          errors.push(format!(
            "judgment for query `{}` references unknown doc_id `{}`",
            judgment.query_id, doc_id
          ));
        }
      }
      if errors.len() >= 5 {
        break;
      }
    }
    if !errors.is_empty() {
      return Err(RAGError::invalid_input(format!(
        "dataset validation failed: {}",
        errors.join("; ")
      )));
    }
    Ok(())
  }

  /// Load a dataset from a directory layout:
  ///
  /// ```text
  /// <dir>/
  ///   dataset.toml      # optional manifest
  ///   corpus.jsonl      # one CorpusDoc per line
  ///   queries.jsonl     # one Query per line
  ///   qrels.jsonl       # one Judgment per line
  /// ```
  pub fn load_from_dir(dir: impl AsRef<Path>) -> Result<Self> {
    let dir = dir.as_ref();
    let corpus = load_jsonl::<CorpusDoc>(&dir.join("corpus.jsonl"))?;
    let queries = load_jsonl::<Query>(&dir.join("queries.jsonl"))?;
    let judgments = load_jsonl::<Judgment>(&dir.join("qrels.jsonl"))?;
    let manifest_path = dir.join("dataset.toml");
    let manifest = if manifest_path.exists() {
      let raw = std::fs::read_to_string(&manifest_path)?;
      // toml is a transitive dep via several workspace crates, but to keep
      // agentflow-rag self-contained the eval module deliberately avoids
      // adding a new direct dep. Manifest is best-effort: if parsing fails
      // we surface it as an invalid_input error rather than silently dropping.
      parse_toml_manifest(&raw)?
    } else {
      DatasetManifest::default()
    };
    let dataset = Self {
      manifest,
      corpus,
      queries,
      judgments,
    };
    dataset.validate()?;
    Ok(dataset)
  }
}

fn load_jsonl<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Vec<T>> {
  if !path.exists() {
    return Err(RAGError::not_found(path.display().to_string()));
  }
  let file = File::open(path)?;
  let reader = BufReader::new(file);
  let mut out = Vec::new();
  for (lineno, line) in reader.lines().enumerate() {
    let line = line?;
    if line.trim().is_empty() {
      continue;
    }
    let item: T = serde_json::from_str(&line).map_err(|err| {
      RAGError::invalid_input(format!(
        "{}:line {}: {}",
        path.display(),
        lineno + 1,
        err
      ))
    })?;
    out.push(item);
  }
  Ok(out)
}

/// Hand-rolled TOML manifest parser — accepts only the four flat string keys
/// we document. Avoids a direct `toml` dep on the leaf crate. Anything more
/// elaborate should live in the manifest itself rather than the parser.
fn parse_toml_manifest(raw: &str) -> Result<DatasetManifest> {
  let mut manifest = DatasetManifest::default();
  for line in raw.lines() {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
      continue;
    }
    let Some((key, value)) = line.split_once('=') else {
      continue;
    };
    let key = key.trim();
    let value = value.trim().trim_matches('"').to_string();
    match key {
      "name" => manifest.name = Some(value),
      "version" => manifest.version = Some(value),
      "source" => manifest.source = Some(value),
      "license" => manifest.license = Some(value),
      "description" => manifest.description = Some(value),
      _ => {}
    }
  }
  Ok(manifest)
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::io::Write;

  #[test]
  fn judgment_iteration_filters_zero() {
    let mut relevances = HashMap::new();
    relevances.insert("d1".into(), 1);
    relevances.insert("d2".into(), 0);
    relevances.insert("d3".into(), 2);
    let j = Judgment {
      query_id: "q1".into(),
      relevances,
      notes: None,
    };
    let mut ids: Vec<&str> = j.relevant_ids().collect();
    ids.sort();
    assert_eq!(ids, vec!["d1", "d3"]);
    assert_eq!(j.relevance("d2"), 0);
    assert!(!j.is_relevant("d2"));
    assert!(j.is_relevant("d3"));
  }

  #[test]
  fn validate_rejects_unknown_doc_id() {
    let mut relevances = HashMap::new();
    relevances.insert("nope".into(), 1);
    let dataset = Dataset::new(
      vec![CorpusDoc {
        id: "d1".into(),
        text: "hello".into(),
        title: None,
      }],
      vec![Query {
        id: "q1".into(),
        text: "h".into(),
        notes: None,
      }],
      vec![Judgment {
        query_id: "q1".into(),
        relevances,
        notes: None,
      }],
    );
    assert!(dataset.validate().is_err());
  }

  #[test]
  fn validate_rejects_unknown_query_id() {
    let mut relevances = HashMap::new();
    relevances.insert("d1".into(), 1);
    let dataset = Dataset::new(
      vec![CorpusDoc {
        id: "d1".into(),
        text: "hello".into(),
        title: None,
      }],
      vec![Query {
        id: "q1".into(),
        text: "h".into(),
        notes: None,
      }],
      vec![Judgment {
        query_id: "ghost".into(),
        relevances,
        notes: None,
      }],
    );
    assert!(dataset.validate().is_err());
  }

  #[test]
  fn load_from_dir_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();

    let corpus_path = dir.join("corpus.jsonl");
    let mut f = File::create(&corpus_path).unwrap();
    writeln!(
      f,
      r#"{{"id":"d1","text":"the quick brown fox"}}"#
    )
    .unwrap();
    writeln!(f, r#"{{"id":"d2","text":"the lazy dog"}}"#).unwrap();
    drop(f);

    let queries_path = dir.join("queries.jsonl");
    let mut f = File::create(&queries_path).unwrap();
    writeln!(f, r#"{{"id":"q1","text":"brown fox"}}"#).unwrap();
    drop(f);

    let qrels_path = dir.join("qrels.jsonl");
    let mut f = File::create(&qrels_path).unwrap();
    writeln!(
      f,
      r#"{{"query_id":"q1","relevances":{{"d1":1}}}}"#
    )
    .unwrap();
    drop(f);

    let manifest_path = dir.join("dataset.toml");
    std::fs::write(
      &manifest_path,
      "name = \"demo\"\nversion = \"0.1\"\nsource = \"unit-test\"\n",
    )
    .unwrap();

    let dataset = Dataset::load_from_dir(dir).unwrap();
    assert_eq!(dataset.corpus.len(), 2);
    assert_eq!(dataset.queries.len(), 1);
    assert_eq!(dataset.judgments.len(), 1);
    assert_eq!(dataset.manifest.name.as_deref(), Some("demo"));
    assert_eq!(dataset.manifest.version.as_deref(), Some("0.1"));
  }

  #[test]
  fn load_from_dir_skips_blank_lines() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    std::fs::write(
      dir.join("corpus.jsonl"),
      "\n{\"id\":\"d1\",\"text\":\"x\"}\n\n",
    )
    .unwrap();
    std::fs::write(
      dir.join("queries.jsonl"),
      "{\"id\":\"q1\",\"text\":\"x\"}\n",
    )
    .unwrap();
    std::fs::write(
      dir.join("qrels.jsonl"),
      "{\"query_id\":\"q1\",\"relevances\":{\"d1\":1}}\n",
    )
    .unwrap();
    let dataset = Dataset::load_from_dir(dir).unwrap();
    assert_eq!(dataset.corpus.len(), 1);
  }
}
