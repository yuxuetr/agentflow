//! A Skill as a [`Capability`] (P-A4.3 — RFC §2 lowering).
//!
//! [`SkillCapability`] bundles a loaded [`SkillManifest`] with its directory and
//! implements [`Capability`]: `lower()` produces the Skill's tools (built-in +
//! MCP + the P-A4.2 `rag_search` tool) and its persona as a context fragment. A
//! surface can then merge this `Lowered` with other capabilities into one
//! registry + context bundle before handing a runtime its inputs — without the
//! runtime ever knowing a Skill was involved.

use std::path::{Path, PathBuf};

use agentflow_agent_spi::capability::{Capability, CapabilityError, Lowered};
use agentflow_agent_spi::harness::context::{ContextItem, ContextPriority};
use async_trait::async_trait;

use crate::builder::{SkillBuilder, build_persona};
use crate::manifest::SkillManifest;

/// `chars / 4` is the same heuristic the harness context providers use; the
/// runtime re-counts with a real tokenizer when budgeting, so this is only a hint.
const CHARS_PER_TOKEN: usize = 4;

fn estimate_tokens(content: &str) -> usize {
  content.chars().count().div_ceil(CHARS_PER_TOKEN).max(1)
}

/// A loaded Skill, ready to be lowered into tools + context.
pub struct SkillCapability {
  manifest: SkillManifest,
  skill_dir: PathBuf,
}

impl SkillCapability {
  /// Bundle a manifest with its skill directory.
  pub fn new(manifest: SkillManifest, skill_dir: impl Into<PathBuf>) -> Self {
    Self {
      manifest,
      skill_dir: skill_dir.into(),
    }
  }

  /// The skill directory this capability lowers from.
  pub fn skill_dir(&self) -> &Path {
    &self.skill_dir
  }

  /// The underlying manifest.
  pub fn manifest(&self) -> &SkillManifest {
    &self.manifest
  }
}

#[async_trait]
impl Capability for SkillCapability {
  async fn lower(&self) -> Result<Lowered, CapabilityError> {
    // Tools: the same registry SkillBuilder assembles for the agent — built-in
    // tools, MCP tools, and the rag-tier `rag_search` tool (P-A4.2).
    let registry = SkillBuilder::build_registry(&self.manifest, &self.skill_dir)
      .await
      .map_err(|e| CapabilityError::Lower(e.to_string()))?;
    let tools = registry.list();

    // Context: the persona (role + language + files-tier knowledge + references)
    // as a single Critical fragment — it is the system instruction, dropped last
    // under budget pressure.
    let persona = build_persona(&self.manifest, &self.skill_dir)
      .map_err(|e| CapabilityError::Lower(e.to_string()))?;
    let context = vec![ContextItem {
      source: format!("skill:{}", self.manifest.skill.name),
      priority: ContextPriority::Critical,
      token_estimate: estimate_tokens(&persona),
      content: persona,
      metadata: serde_json::Value::Null,
    }];

    Ok(Lowered { tools, context })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::loader::SkillLoader;
  use std::fs;
  use std::io::Write;
  use tempfile::TempDir;

  fn write_toml(dir: &Path, content: &str) {
    let mut f = fs::File::create(dir.join("skill.toml")).expect("create skill.toml");
    f.write_all(content.as_bytes()).expect("write");
  }

  fn write_file(path: &Path, content: &str) {
    if let Some(p) = path.parent() {
      fs::create_dir_all(p).expect("mkdir");
    }
    let mut f = fs::File::create(path).expect("create");
    f.write_all(content.as_bytes()).expect("write");
  }

  #[tokio::test]
  async fn lower_yields_persona_context_fragment() {
    let dir = TempDir::new().unwrap();
    write_toml(
      dir.path(),
      r#"
[skill]
name = "lowerable"
version = "0.1"
description = "lowers"

[persona]
role = "You are a meticulous reviewer."
"#,
    );
    let manifest = SkillLoader::load(dir.path()).unwrap();
    let cap = SkillCapability::new(manifest, dir.path());

    let lowered = cap.lower().await.expect("lower ok");
    assert_eq!(lowered.context.len(), 1, "persona is one context fragment");
    let frag = &lowered.context[0];
    assert_eq!(frag.source, "skill:lowerable");
    assert_eq!(frag.priority, ContextPriority::Critical);
    assert!(frag.content.contains("meticulous reviewer"));
    assert!(frag.token_estimate >= 1);
  }

  #[tokio::test]
  async fn lower_surfaces_rag_search_tool_for_rag_tier_knowledge() {
    let dir = TempDir::new().unwrap();
    write_file(
      &dir.path().join("knowledge").join("corpus.md"),
      "Searchable reference corpus.",
    );
    write_toml(
      dir.path(),
      r#"
[skill]
name = "with-rag"
version = "0.1"
description = "rag tier"

[persona]
role = "You are a helper."

[[knowledge]]
path = "./knowledge/corpus.md"
backend = "rag"
"#,
    );
    let manifest = SkillLoader::load(dir.path()).unwrap();
    let cap = SkillCapability::new(manifest, dir.path());

    let lowered = cap.lower().await.expect("lower ok");
    assert!(
      lowered.tools.iter().any(|t| t.name() == "rag_search"),
      "rag-tier knowledge must lower to a rag_search tool"
    );
    // rag-tier content stays out of the lowered persona fragment.
    assert!(
      !lowered.context[0]
        .content
        .contains("Searchable reference corpus"),
      "rag-tier content must not be inlined into the persona fragment"
    );
  }
}
